use super::{HostClient, HostRecord, Ordering, Phase, p, ssh, store::ConnectionRoute, tls};
use std::{collections::BTreeSet, future::Future};
use tonic::transport::{Channel, Identity};

pub(super) fn normalize(record: &mut HostRecord) -> Result<(), String> {
    if record.routes.is_empty() {
        record.routes.push(ConnectionRoute::new(
            if record.ssh.is_some() {
                "SSH"
            } else {
                "Local network / VPN"
            }
            .into(),
            record.endpoint.clone(),
            record.ssh.clone(),
        )?);
    }
    if record.routes.len() > 8 {
        return Err("A computer can have up to eight connection routes.".into());
    }
    let mut ids = BTreeSet::new();
    for route in &record.routes {
        route.validate()?;
        if !ids.insert(route.id()) {
            return Err("Saved connection routes contain a duplicate identity.".into());
        }
    }
    // Keep the original v1 fields readable by builds with one-route support.
    record.endpoint = record.routes[0].endpoint().into();
    record.ssh = record.routes[0].ssh().cloned();
    Ok(())
}

/// Read-only connection attempts, never mutation replay. Cancellation is checked
/// before and after each attempt, including a successful late completion.
pub(super) async fn try_routes<T, F, Fut>(
    routes: &[ConnectionRoute],
    cancelled: impl Fn() -> bool,
    mut attempt: F,
) -> Result<(String, T), String>
where
    F: FnMut(&ConnectionRoute) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut failures = Vec::new();
    for route in routes {
        if cancelled() {
            return Err("Connection cancelled.".into());
        }
        let result = attempt(route).await;
        if cancelled() {
            return Err("Connection cancelled.".into());
        }
        match result {
            Ok(value) => return Ok((route.id().into(), value)),
            Err(error) => failures.push(format!(
                "{}: {}",
                route.name(),
                error.chars().take(512).collect::<String>()
            )),
        }
    }
    Err(format!(
        "Could not reach this computer. {}",
        failures.join(" · ")
    ))
}

type ConnectedRoute = (Channel, Option<ssh::SshTunnel>, p::HostInfo, p::Catalog);
impl HostClient {
    pub fn routes(&self) -> Vec<ConnectionRoute> {
        self.state.lock().unwrap().record.routes.clone()
    }
    pub fn active_route(&self) -> Option<String> {
        self.state.lock().unwrap().active_route.clone()
    }
    pub fn route_error(&self, id: &str) -> Option<String> {
        self.state.lock().unwrap().route_errors.get(id).cloned()
    }

    /// Edit is CAS against the route shown by the form; another window's changes
    /// cannot be overwritten. Adding a route preserves the running connection.
    pub fn save_route(
        &self,
        route: ConnectionRoute,
        previous: Option<&ConnectionRoute>,
    ) -> Result<(), String> {
        route.validate()?;
        let removed = self.update_record(|record| {
            let at = record.routes.iter().position(|r| r.id() == route.id());
            let old = at.map(|at| record.routes[at].clone());
            if old.as_ref() != previous {
                return Err(
                    "This route changed in another window. Open it again before saving.".into(),
                );
            }
            if let Some(at) = at {
                record.routes[at] = route.clone();
            } else {
                record.routes.push(route.clone());
            }
            normalize(record)?;
            Ok(old)
        })?;
        if self.active_route().as_deref() == Some(route.id()) || self.phase() == Phase::Connecting {
            self.disconnect();
        }
        self.cleanup_route_secret(removed.as_ref());
        self.changed();
        Ok(())
    }
    pub fn remove_route(&self, id: &str) -> Result<(), String> {
        let old = self.update_record(|record| {
            if record.routes.len() <= 1 {
                return Err("Keep at least one route so this computer remains reachable.".into());
            }
            let at = record
                .routes
                .iter()
                .position(|r| r.id() == id)
                .ok_or("Route no longer exists.")?;
            let old = record.routes.remove(at);
            normalize(record)?;
            Ok(old)
        })?;
        if self.active_route().as_deref() == Some(id) || self.phase() == Phase::Connecting {
            self.disconnect();
        }
        self.cleanup_route_secret(Some(&old));
        self.changed();
        Ok(())
    }
    pub fn move_route(&self, id: &str, position: usize) -> Result<(), String> {
        self.update_record(|record| {
            let at = record
                .routes
                .iter()
                .position(|r| r.id() == id)
                .ok_or("Route no longer exists.")?;
            let route = record.routes.remove(at);
            record
                .routes
                .insert(position.min(record.routes.len()), route);
            normalize(record)
        })?;
        self.changed();
        Ok(())
    }
    fn cleanup_route_secret(&self, old: Option<&ConnectionRoute>) {
        let Some(ssh) = old.and_then(ConnectionRoute::ssh) else {
            return;
        };
        if self.routes().iter().any(|r| {
            r.ssh()
                .is_some_and(|r| r.credential_ref == ssh.credential_ref)
        }) {
            return;
        }
        if ssh::remove_credentials(self.vault.as_ref(), &ssh.credential_ref).is_err() {
            self.state.lock().unwrap().error = Some(
                "Routes saved, but an unused SSH credential could not be removed from the vault."
                    .into(),
            );
        }
    }
    pub(super) async fn open_route(
        &self,
        record: &HostRecord,
        route: &ConnectionRoute,
        identity: Identity,
        generation: u64,
    ) -> Result<ConnectedRoute, String> {
        let endpoint =
            url::Url::parse(route.endpoint()).map_err(|_| "Invalid connection address.")?;
        let first_hop = route
            .ssh()
            .map(|ssh| (ssh.address.clone(), ssh.port))
            .unwrap_or_else(|| {
                (
                    endpoint.host_str().unwrap_or_default().to_owned(),
                    endpoint.port_or_known_default().unwrap_or(443),
                )
            });
        let reachable = tokio::task::spawn_blocking(move || {
            crate::remote::network::reachable_addresses(&first_hop.0, first_hop.1)
        })
        .await
        .map_err(|_| "Network route lookup stopped.")??;
        if self.generation.load(Ordering::Acquire) != generation {
            return Err("Connection cancelled.".into());
        }
        let tunnel = if let Some(ssh_route) = route.ssh().cloned() {
            let mut ssh_route = ssh_route;
            ssh_route.address = reachable[0].to_string();
            let vault = self.vault.clone();
            let address = route.endpoint().to_owned();
            let return_port = self.peer.lock().unwrap().as_ref().map(|peer| peer.port());
            Some(
                tokio::task::spawn_blocking(move || {
                    ssh::SshTunnel::open_with_return(ssh_route, &address, vault, return_port)
                })
                .await
                .map_err(|_| "SSH worker failed.")??,
            )
        } else {
            None
        };
        if self.generation.load(Ordering::Acquire) != generation {
            return Err("Connection cancelled.".into());
        }
        let (address, reachable) = if let Some(tunnel) = &tunnel {
            (tunnel.address(), vec!["127.0.0.1".parse().unwrap()])
        } else {
            (endpoint.as_str(), reachable)
        };
        let channel = connect_tls(
            address,
            reachable,
            &record.server_name,
            &record.certificate,
            &record.fingerprint,
            Some(identity),
            || self.generation.load(Ordering::Acquire) != generation,
        )
        .await?;
        let info = p::host_service_client::HostServiceClient::new(channel.clone())
            .get_info(p::HostRequest {
                host_id: self.id.0.clone(),
            })
            .await
            .map_err(super::rpc_error)?
            .into_inner();
        if info.host_id != self.id.0 || info.protocol_version != 1 {
            return Err("Host identity or protocol mismatch.".into());
        }
        let catalog = super::catalog::read_catalog(channel.clone(), &self.id.0).await?;
        if catalog.host_id != self.id.0 {
            return Err("Catalog host identity mismatch.".into());
        }
        Ok((channel, tunnel, info, catalog))
    }
}

/// Only dial addresses admitted by the OS route lookup. Keeping SNI and the pin
/// separate permits IPv4/IPv6 DNS fallback without trusting another identity.
pub(super) async fn connect_tls(
    endpoint: &str,
    addresses: Vec<std::net::IpAddr>,
    server_name: &str,
    certificate: &[u8],
    fingerprint: &[u8],
    identity: Option<Identity>,
    cancelled: impl Fn() -> bool,
) -> Result<Channel, String> {
    let mut endpoint = url::Url::parse(endpoint).map_err(|_| "Invalid connection address.")?;
    for address in addresses {
        if cancelled() {
            return Err("Connection cancelled.".into());
        }
        endpoint
            .set_ip_host(address)
            .map_err(|_| "Invalid connection address.")?;
        let result = tls::client_endpoint(
            endpoint.as_str(),
            server_name,
            certificate,
            fingerprint,
            identity.clone(),
        )?
        .connect_timeout(std::time::Duration::from_secs(4))
        .connect()
        .await;
        if cancelled() {
            return Err("Connection cancelled.".into());
        }
        if let Ok(channel) = result {
            return Ok(channel);
        }
    }
    Err("No secure connection. Check that Canopy is running and this route is reachable.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::client::{
        ClientRegistry,
        tests::{FakeVault, record},
    };
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn route_changes_keep_pairing_and_drafts_and_survive_restore() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("client.json");
        let registry = ClientRegistry::open(path.clone(), Arc::new(FakeVault)).unwrap();
        let mut record = record();
        record
            .views
            .insert("chat:draft".into(), "Keep this draft".into());
        let identity = (
            record.id.clone(),
            record.credential_ref.clone(),
            record.fingerprint.clone(),
        );
        // Existing one-route records become a list, without pairing again.
        let host = registry.insert(record).unwrap();
        let original = host.routes()[0].clone();
        let office =
            ConnectionRoute::new("Office".into(), "https://192.168.128.30:44330".into(), None)
                .unwrap();
        let vpn = ConnectionRoute::new(
            "Tailscale".into(),
            "https://100.100.255.90:44330".into(),
            None,
        )
        .unwrap();
        host.save_route(office.clone(), None).unwrap();
        host.save_route(vpn.clone(), None).unwrap();
        host.move_route(vpn.id(), 0).unwrap();
        host.remove_route(original.id()).unwrap();
        let restored = ClientRegistry::open(path.clone(), Arc::new(FakeVault))
            .unwrap()
            .get(&identity.0)
            .unwrap();
        assert_eq!(
            restored
                .routes()
                .iter()
                .map(|r| r.name())
                .collect::<Vec<_>>(),
            ["Tailscale", "Office"]
        );
        assert_eq!(restored.routes()[0].id(), vpn.id());
        let snapshot = restored.snapshot();
        assert_eq!(snapshot.record.credential_ref, identity.1);
        assert_eq!(snapshot.record.fingerprint, identity.2);
        assert_eq!(snapshot.record.view("chat:draft"), Some("Keep this draft"));
        // A stale form cannot undo another window's route edit.
        let edited = vpn
            .edited("VPN".into(), vpn.endpoint().into(), None)
            .unwrap();
        restored.save_route(edited.clone(), Some(&vpn)).unwrap();
        assert!(restored.save_route(vpn.clone(), Some(&vpn)).is_err());
        // A failed durable write preserves the currently usable route list.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(restored.remove_route(vpn.id()).is_err());
        assert_eq!(restored.routes()[0].name(), "VPN");
        assert_eq!(restored.routes().len(), 2);
    }

    #[tokio::test]
    async fn route_fallback_stops_at_success_and_cancellation_fences_late_results() {
        let office =
            ConnectionRoute::new("Office".into(), "https://192.168.128.30:44330".into(), None)
                .unwrap();
        let home =
            ConnectionRoute::new("Home".into(), "https://192.168.1.30:44330".into(), None).unwrap();
        let vpn = ConnectionRoute::new("VPN".into(), "https://100.100.255.90:44330".into(), None)
            .unwrap();
        let routes = [office, home.clone(), vpn];
        let mut attempted = vec![];
        let result = try_routes(
            &routes,
            || false,
            |route| {
                attempted.push(route.name().to_owned());
                std::future::ready(if route.id() == home.id() {
                    Ok("same pinned host")
                } else {
                    Err("unreachable".into())
                })
            },
        )
        .await
        .unwrap();
        assert_eq!(result.0, home.id());
        assert_eq!(attempted, ["Office", "Home"]);
        let cancelled = AtomicBool::new(false);
        let mut attempted = vec![];
        let result = try_routes(
            &routes,
            || cancelled.load(Ordering::Acquire),
            |route| {
                attempted.push(route.name().to_owned());
                cancelled.store(true, Ordering::Release);
                std::future::ready(Ok("late connection"))
            },
        )
        .await;
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(attempted, ["Office"]);
    }
}
