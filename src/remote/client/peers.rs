use super::*;
use crate::{
    command_bus::PeerConnection,
    remote::peer::{LocalPeer, device_id},
};
use std::net::IpAddr;

impl HostClient {
    /// Reconnection can establish reciprocal trust before publishing its channel.
    /// The exact command still belongs to the same durable journal and gate.
    pub(crate) async fn connect_peer_on(
        &self,
        channel: Channel,
        mut request: crate::api::proto::ConnectPeerRequest,
        epoch: u64,
    ) -> Result<bool, String> {
        use super::{
            api_commands::ApiIntent,
            commands::{check_receipt, terminal_state},
            store::MutationRecord,
        };
        let _gate = self.mutation.lock().await;
        let record = self.snapshot().record;
        if record.closing_epoch.is_some() {
            return Ok(record.mutual);
        }
        let pending = record.mutations.iter().find(|entry| {
            entry
                .receipt
                .as_ref()
                .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok())
                .is_none_or(|receipt| !receipt.resolved_by_user && !terminal_state(receipt.state))
        });
        let mut intent = if let Some(entry) = pending {
            let Some((kind, bytes)) = &entry.api_intent else {
                return Ok(record.mutual);
            };
            // Other unresolved operations must remain inspectable after connection.
            if kind != "peer.connect" || entry.epoch != epoch {
                return Ok(record.mutual);
            }
            ApiIntent::decode(kind, bytes)?
        } else {
            request.command = Some(crate::api::proto::CommandMeta {
                command_id: uuid::Uuid::new_v4().to_string(),
                command_epoch: epoch,
                expected_revision: None,
            });
            let mut intent = ApiIntent::ConnectPeer(request);
            let meta = intent
                .meta()
                .clone()
                .ok_or("Missing peer command identity.")?;
            let (kind, bytes) = intent.encode();
            if bytes.len() > crate::host::CONTROL_BYTES {
                return Err("Peer command exceeds the control limit.".into());
            }
            self.update_record(|record| {
                // The gate and pending check above prove every prior entry is
                // resolved. Match ordinary typed admission's bounded journal.
                record.mutations.clear();
                record.mutations.push(MutationRecord {
                    id: meta.command_id.clone(),
                    epoch,
                    receipt: None,
                    chat_intent: None,
                    api_intent: Some((kind.into(), bytes)),
                });
                record.epoch = Some(epoch);
                Ok(())
            })?;
            for entry in record.mutations {
                if let Some((kind, bytes)) = entry.api_intent {
                    self.remove_settings_intent(&kind, &bytes);
                }
            }
            intent
        };
        let meta = intent
            .meta()
            .clone()
            .ok_or("Missing peer command identity.")?;
        let receipt = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            intent.dispatch(channel, self.id().as_str(), String::new()),
        )
        .await
        .map_err(|_| "Peer outcome is uncertain; the original command is retained.".to_owned())??;
        self.store_api_receipt(&meta.command_id, &receipt)?;
        let receipt = check_receipt(receipt)?;
        Ok(receipt.state == p::OperationState::Succeeded as i32)
    }
}

impl ClientRegistry {
    pub fn set_local_peer(&self, peer: Option<Arc<LocalPeer>>) {
        *self.inner.peer.lock().unwrap() = peer;
    }
    pub(crate) fn install_peer(
        &self,
        local: &LocalPeer,
        peer: PeerConnection,
        observed: Option<IpAddr>,
    ) -> Result<Arc<HostClient>, String> {
        let _writer = self
            .inner
            .peer_write
            .lock()
            .map_err(|_| "Peer storage unavailable.")?;
        let certificate = peer.device_certificate;
        let chain = peer.certificate_chain;
        let id = HostId::parse(peer.host.as_str())?;
        if peer.host.as_str() == local.host_id()
            || peer.name.trim().is_empty()
            || peer.name.len() > 256
            || peer.name.chars().any(char::is_control)
            || peer.server_name != format!("canopy-{}.local", peer.host)
            || peer.port == 0
            || peer.addresses.len() > 8
            || peer.certificate.len() > 16384
            || certificate.len() > 16384
            || chain.is_empty()
            || chain.len() > 8
            || chain.iter().map(Vec::len).sum::<usize>() > 16384
            || (peer.return_tunnel && !observed.is_some_and(|ip| ip.is_loopback()))
            || peer.fingerprint != sha2::Sha256::digest(&peer.certificate).as_slice()
        {
            return Err("Invalid reciprocal computer identity.".into());
        }
        let key = local.key();
        local.validate_certificate(&certificate)?;
        let mut addresses = Vec::new();
        if !peer.return_tunnel
            && let Some(ip) = observed.filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        {
            addresses.push(ip);
        }
        for address in peer.addresses {
            let ip: IpAddr = address
                .parse()
                .map_err(|_| "Invalid peer network address.")?;
            if (ip.is_loopback() && !peer.return_tunnel) || ip.is_unspecified() || ip.is_multicast()
            {
                return Err("Invalid peer network address.".into());
            }
            if !addresses.contains(&ip) && addresses.len() < 8 {
                addresses.push(ip);
            }
        }
        if addresses.is_empty() {
            addresses.extend(observed.filter(|ip| ip.is_loopback()));
        }
        let routes = addresses
            .into_iter()
            .map(|ip| {
                let address = std::net::SocketAddr::new(ip, peer.port);
                store::ConnectionRoute::new(
                    if peer.return_tunnel {
                        "SSH return".into()
                    } else {
                        format!("Network · {ip}")
                    },
                    format!("https://{address}/"),
                    None,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let endpoint = routes
            .first()
            .ok_or("No address for the reciprocal connection.")?
            .endpoint()
            .to_owned();
        tls::client_endpoint(
            &endpoint,
            &peer.server_name,
            &peer.certificate,
            &peer.fingerprint,
            None,
        )?;
        if let Some(existing) = self.get(&id) {
            existing.update_record(|record| {
                if record.certificate != peer.certificate || record.fingerprint != peer.fingerprint {
                    return Err("The saved computer identity differs. Remove the old connection before pairing again.".into());
                }
                record.mutual = true;
                record.passive_peer = true;
                for route in &routes {
                    if peer.return_tunnel && let Some(index) = record.routes.iter().position(|old| old.name() == "SSH return") {
                        record.routes[index] = record.routes[index].edited("SSH return".into(), route.endpoint().into(), None)?;
                    } else if !record.routes.iter().any(|old| old.endpoint() == route.endpoint()) && record.routes.len() < 8 {
                        record.routes.push(route.clone());
                    }
                }
                Ok(())
            })?;
            existing.changed();
            return Ok(existing);
        }
        // Publication may have succeeded before directory fsync reported an
        // error. Reconcile that record before touching immutable vault slots.
        let committed = self.inner.store.lock().unwrap().committed_host(&id)?;
        if let Some(committed) = committed {
            if committed.certificate != peer.certificate
                || committed.fingerprint != peer.fingerprint
            {
                return Err("The committed computer identity differs.".into());
            }
            return self.insert(committed);
        }
        if self.hosts().len() >= 32 {
            return Err("The saved-computer limit has been reached.".into());
        }
        // Retry after a disk failure uses the same staging slots, rather than
        // leaking another private key bundle into the vault on every attempt.
        let reference = device_id(local.host_id(), &format!("credential:{}", peer.host));
        let pem = std::iter::once(&certificate)
            .chain(&chain)
            .map(|der| pairing::pem_certificate(der))
            .collect::<String>();
        pairing::save_bundle(self.inner.vault.as_ref(), &reference, key, &pem)?;
        let record = HostRecord {
            id,
            name: peer.name,
            endpoint,
            server_name: peer.server_name,
            certificate: peer.certificate,
            fingerprint: peer.fingerprint,
            device_id: device_id(peer.host.as_str(), local.host_id()),
            credential_ref: reference.clone(),
            mutual: true,
            passive_peer: true,
            auto_connect: true,
            ssh: None,
            routes,
            last_seen: None,
            epoch: None,
            closing_epoch: None,
            catalog: vec![],
            outbox: vec![],
            mutations: vec![],
            views: BTreeMap::new(),
            uploads: vec![],
            chats: BTreeMap::new(),
        };
        match self.insert(record) {
            Ok(host) => Ok(host),
            Err(error) => {
                // An fsync acknowledgement can fail after publication. Never
                // delete keys when the durable record may already reference them.
                // A retry reuses that record; no older credential is overwritten.
                Err(format!(
                    "{error} Peer credentials were retained for recovery."
                ))
            }
        }
    }
}

use sha2::Digest;

pub(crate) fn start_connection(host: Arc<HostClient>) {
    if host.reconnecting.swap(true, Ordering::AcqRel) {
        return;
    }
    struct Guard(Arc<AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let guard = Guard(host.reconnecting.clone());
    let events = host.events();
    let weak = Arc::downgrade(&host);
    tokio::spawn(async move {
        let _guard = guard;
        let mut delay = 1;
        loop {
            let Some(host) = weak.upgrade() else {
                return;
            };
            if !host.auto_connect() {
                return;
            }
            if host.phase() != Phase::Offline {
                drop(host);
                if events.recv().await.is_err() {
                    return;
                }
                delay = 1;
                continue;
            }
            if host.connect().await.is_err() {
                drop(host);
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                delay = (delay * 2).min(60);
            }
        }
    });
}
