//! Device-owned native remote client. Host identity never follows the selected tab or route.
pub mod catalog;
mod chat_commands;
mod commands;
pub use chat_commands::ChatMutation;
pub mod pairing;
mod projection;
mod resources;
mod routes;
pub mod ssh;
pub mod store;
use crate::remote::{proto as p, tls};
pub use projection::ChatProjection;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use store::{CredentialStore, HostRecord, Store};
use tonic::transport::Channel;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostId(String);
impl HostId {
    pub fn parse(value: &str) -> Result<Self, String> {
        uuid::Uuid::parse_str(value).map_err(|_| "Invalid host identity.")?;
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone)]
pub struct ClientRegistry {
    inner: Arc<Registry>,
}
struct Registry {
    store: Arc<Mutex<Store>>,
    vault: Arc<dyn CredentialStore>,
    hosts: Mutex<BTreeMap<HostId, Arc<HostClient>>>,
    listeners: Mutex<Vec<async_channel::Sender<()>>>,
}
impl ClientRegistry {
    /// Call on a worker, never render. The path belongs to this device's data directory.
    pub fn open(path: PathBuf, vault: Arc<dyn CredentialStore>) -> Result<Self, String> {
        let store = Store::open(path)?;
        let records = store.hosts();
        let registry = Self {
            inner: Arc::new(Registry {
                store: Arc::new(Mutex::new(store)),
                vault,
                hosts: Mutex::new(BTreeMap::new()),
                listeners: Mutex::new(Vec::new()),
            }),
        };
        for record in records {
            registry.insert(record)?;
        }
        Ok(registry)
    }
    pub fn hosts(&self) -> Vec<Arc<HostClient>> {
        self.inner.hosts.lock().unwrap().values().cloned().collect()
    }
    pub fn get(&self, id: &HostId) -> Option<Arc<HostClient>> {
        self.inner.hosts.lock().unwrap().get(id).cloned()
    }
    pub fn vault(&self) -> Arc<dyn CredentialStore> {
        self.inner.vault.clone()
    }
    pub fn events(&self) -> async_channel::Receiver<()> {
        let (sender, receiver) = async_channel::bounded(1);
        self.inner.listeners.lock().unwrap().push(sender);
        receiver
    }
    pub fn insert(&self, mut record: HostRecord) -> Result<Arc<HostClient>, String> {
        let mut hosts = self.inner.hosts.lock().unwrap();
        if let Some(existing) = hosts.get(&record.id) {
            return Ok(existing.clone());
        }
        routes::normalize(&mut record)?;
        self.inner.store.lock().unwrap().save(record.clone())?;
        let host = Arc::new(HostClient {
            id: record.id.clone(),
            state: Mutex::new(HostState {
                catalog: p::Catalog::decode(record.catalog.as_slice())
                    .ok()
                    .map(Arc::new),
                record,
                phase: Phase::Offline,
                error: None,
                channel: None,
                tunnel: None,
                info: None,
                watch: None,
                view_revisions: BTreeMap::new(),
                active_route: None,
                route_errors: BTreeMap::new(),
            }),
            store: self.inner.store.clone(),
            vault: self.inner.vault.clone(),
            generation: AtomicU64::new(0),
            mutation: tokio::sync::Mutex::new(()),
            listeners: Mutex::new(Vec::new()),
        });
        hosts.insert(host.id.clone(), host.clone());
        drop(hosts);
        self.inner.listeners.lock().unwrap().retain(|listener| {
            !matches!(
                listener.try_send(()),
                Err(async_channel::TrySendError::Closed(_))
            )
        });
        Ok(host)
    }
    pub fn disconnect_all(&self) {
        for host in self.hosts() {
            host.disconnect();
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Offline,
    Connecting,
    Ready,
}
#[derive(Clone)]
pub struct HostSnapshot {
    pub record: HostRecord,
    pub phase: Phase,
    pub error: Option<String>,
    pub catalog: Option<p::Catalog>,
    pub info: Option<p::HostInfo>,
}
struct HostState {
    record: HostRecord,
    phase: Phase,
    error: Option<String>,
    channel: Option<Channel>,
    tunnel: Option<ssh::SshTunnel>,
    catalog: Option<Arc<p::Catalog>>,
    info: Option<p::HostInfo>,
    watch: Option<tokio::task::AbortHandle>,
    view_revisions: BTreeMap<String, u64>,
    active_route: Option<String>,
    route_errors: BTreeMap<String, String>,
}
pub struct HostClient {
    id: HostId,
    state: Mutex<HostState>,
    store: Arc<Mutex<Store>>,
    vault: Arc<dyn CredentialStore>,
    generation: AtomicU64,
    mutation: tokio::sync::Mutex<()>,
    listeners: Mutex<Vec<async_channel::Sender<()>>>,
}
impl HostClient {
    pub fn id(&self) -> &HostId {
        &self.id
    }
    pub fn phase(&self) -> Phase {
        self.state.lock().unwrap().phase
    }
    pub fn name(&self) -> String {
        self.state.lock().unwrap().record.name.clone()
    }
    pub fn endpoint(&self) -> String {
        self.state.lock().unwrap().record.endpoint.clone()
    }
    pub fn last_seen(&self) -> Option<i64> {
        self.state.lock().unwrap().record.last_seen
    }
    pub fn connection_error(&self) -> Option<String> {
        self.state.lock().unwrap().error.clone()
    }
    pub fn catalog(&self) -> Option<p::Catalog> {
        self.state.lock().unwrap().catalog.as_deref().cloned()
    }
    pub fn shared_catalog(&self) -> Option<Arc<p::Catalog>> {
        self.state.lock().unwrap().catalog.clone()
    }
    pub fn snapshot(&self) -> HostSnapshot {
        let state = self.state.lock().unwrap();
        HostSnapshot {
            record: state.record.clone(),
            phase: state.phase,
            error: state.error.clone(),
            catalog: state.catalog.as_deref().cloned(),
            info: state.info.clone(),
        }
    }
    pub fn events(&self) -> async_channel::Receiver<()> {
        let (sender, receiver) = async_channel::bounded(1);
        self.listeners.lock().unwrap().push(sender);
        receiver
    }
    fn changed(&self) {
        self.listeners.lock().unwrap().retain(|listener| {
            !matches!(
                listener.try_send(()),
                Err(async_channel::TrySendError::Closed(_))
            )
        });
    }
    pub fn channel(&self) -> Result<Channel, String> {
        self.state
            .lock()
            .unwrap()
            .channel
            .clone()
            .ok_or("Host is offline. Reconnect before making changes.".into())
    }
    pub fn disconnect(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let mut state = self.state.lock().unwrap();
        if let Some(task) = state.watch.take() {
            task.abort();
        }
        state.channel = None;
        state.tunnel = None;
        state.phase = Phase::Offline;
        state.active_route = None;
        drop(state);
        self.changed();
    }
    pub fn connect(
        self: &Arc<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>>
    {
        let host = self.clone();
        Box::pin(async move { host.connect_once(None).await })
    }
    pub async fn connect_via(self: &Arc<Self>, route: String) -> Result<(), String> {
        self.connect_once(Some(route)).await
    }
    async fn connect_once(self: &Arc<Self>, route: Option<String>) -> Result<(), String> {
        self.disconnect();
        let generation = self.generation.load(Ordering::Acquire);
        {
            let mut state = self.state.lock().unwrap();
            state.phase = Phase::Connecting;
            state.error = None;
            state.route_errors.clear();
        }
        self.changed();
        let result = self.connect_inner(generation, route.as_deref()).await;
        if let Err(error) = &result
            && self.generation.load(Ordering::Acquire) == generation
        {
            let mut state = self.state.lock().unwrap();
            state.phase = Phase::Offline;
            state.error = Some(error.clone());
            drop(state);
            self.changed();
        }
        result
    }
    async fn connect_inner(
        self: &Arc<Self>,
        generation: u64,
        selected: Option<&str>,
    ) -> Result<(), String> {
        let record = self.snapshot().record;
        let vault = self.vault.clone();
        let reference = record.credential_ref.clone();
        let identity =
            tokio::task::spawn_blocking(move || pairing::identity(vault.as_ref(), &reference))
                .await
                .map_err(|_| "Credential worker failed.")??;
        let candidates = record
            .routes
            .iter()
            .filter(|r| selected.is_none_or(|id| r.id() == id))
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err("Connection route no longer exists.".into());
        }
        let connected = routes::try_routes(
            &candidates,
            || self.generation.load(Ordering::Acquire) != generation,
            |route| {
                let identity = identity.clone();
                let route = route.clone();
                let record = &record;
                async move {
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        self.open_route(record, &route, identity, generation),
                    )
                    .await
                    .unwrap_or_else(|_| Err("This route did not respond in time.".into()));
                    if let Err(error) = &result
                        && self.generation.load(Ordering::Acquire) == generation
                    {
                        self.state
                            .lock()
                            .unwrap()
                            .route_errors
                            .insert(route.id().into(), error.chars().take(512).collect());
                        self.changed();
                    }
                    result
                }
            },
        )
        .await?;
        let (route_id, (channel, tunnel, info, catalog)) = connected;
        if generation != self.generation.load(Ordering::Acquire) {
            return Err("Connection cancelled.".into());
        }
        self.update_record(|record| {
            record.last_seen = Some(chrono::Utc::now().timestamp());
            record.epoch = Some(info.command_epoch);
            record.catalog = catalog.encode_to_vec();
            Ok(())
        })?;
        {
            let mut state = self.state.lock().unwrap();
            if self.generation.load(Ordering::Acquire) != generation {
                return Err("Connection cancelled.".into());
            }
            state.channel = Some(channel.clone());
            state.tunnel = tunnel;
            state.catalog = Some(Arc::new(catalog));
            state.info = Some(info);
            state.phase = Phase::Ready;
            state.active_route = Some(route_id);
            state.error = None;
        }
        self.changed();
        let weak = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            let result = async {
                let mut health = p::host_service_client::HostServiceClient::new(channel.clone())
                    .watch_host(p::HostRequest { host_id: record.id.0.clone() }).await.map_err(rpc_error)?.into_inner();
                let mut stream = p::catalog_service_client::CatalogServiceClient::new(channel)
                    .watch_summaries(p::SubscribeRequest {
                        host_id: record.id.0.clone(),
                        since: None,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                let mut feed = catalog::CatalogFeed::default();
                loop {
                    let event = tokio::select! {
                        event = stream.message() => event.map_err(rpc_error)?.ok_or("Catalog stream disconnected.")?,
                        status = health.message() => {
                            let status = status.map_err(rpc_error)?.ok_or("Host status stream disconnected.")?;
                            if status.host_id != record.id.0 { return Err("Host status identity mismatch.".into()); }
                            if status.readiness != 2 { return Err(format!("Host is not ready: {}", status.reason)); }
                            continue;
                        }
                    };
                    let Some(host) = weak.upgrade() else {
                        return Ok::<_, String>(());
                    };
                    if host.generation.load(Ordering::Acquire) != generation {
                        return Ok(());
                    }
                    let catalog = {
                        let mut state = host.state.lock().unwrap();
                        if state.catalog.is_none() {
                            return Err("Missing catalog.".into());
                        }
                        // Pages of a snapshot are staged; the catalog changes only
                        // with a whole snapshot or a delta after it.
                        let started = std::time::Instant::now();
                        let applied = feed.apply(&host.id.0, &mut state.catalog, event);
                        crate::remote::metrics::record(
                            &host.id.0,
                            &crate::remote::metrics::resource_id("catalog"),
                            None,
                            generation,
                            crate::remote::metrics::Stage::ClientReceiveApply,
                            started,
                            if applied.is_ok() {
                                crate::remote::metrics::Outcome::Ok
                            } else {
                                crate::remote::metrics::Outcome::Invalid
                            },
                        );
                        if !applied? {
                            continue;
                        }
                        state.catalog.as_deref().map(Message::encode_to_vec).unwrap_or_default()
                    };
                    host.update_record(|record| {
                        record.catalog = catalog;
                        record.last_seen = Some(chrono::Utc::now().timestamp());
                        Ok(())
                    })?;
                    host.changed();
                }
            }
            .await;
            if let Some(host) = weak.upgrade()
                && host.generation.load(Ordering::Acquire) == generation
            {
                {
                    let mut state = host.state.lock().unwrap();
                    state.watch = None;
                    state.channel = None;
                    state.tunnel = None;
                    state.phase = Phase::Offline;
                    state.active_route = None;
                    state.error = result.err();
                }
                host.changed();
                let mut expected = generation;
                for delay in [1, 2, 4, 8, 16] {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    if host.generation.load(Ordering::Acquire) != expected {
                        return;
                    }
                    if host.connect().await.is_ok() {
                        return;
                    }
                    expected = host.generation.load(Ordering::Acquire);
                }
            }
        });
        let mut state = self.state.lock().unwrap();
        if self.generation.load(Ordering::Acquire) != generation {
            task.abort();
            return Err("Connection cancelled.".into());
        }
        state.watch = Some(task.abort_handle());
        Ok(())
    }
    fn update_record<T>(
        &self,
        edit: impl FnOnce(&mut HostRecord) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.state.lock().unwrap();
        let mut record = state.record.clone();
        let result = edit(&mut record)?;
        self.store.lock().unwrap().save(record.clone())?;
        state.record = record;
        Ok(result)
    }
    pub fn set_display_name(&self, name: String) -> Result<(), String> {
        if name.trim().is_empty() || name.len() > 128 {
            return Err("Host label must contain 1–128 bytes.".into());
        }
        self.update_record(|record| {
            record.name = name;
            Ok(())
        })?;
        self.changed();
        Ok(())
    }

    pub fn cached_chat(&self, chat: &str) -> Option<ChatProjection> {
        let state = self.state.lock().unwrap();
        let cached = state.record.chats.get(chat)?;
        Some(ChatProjection::from_cache(cached))
    }
    pub fn cache_chat(&self, chat: &str, projection: &ChatProjection) -> Result<(), String> {
        let cached = store::CachedChat {
            items: projection
                .items
                .iter()
                .map(Message::encode_to_vec)
                .collect(),
            summary: projection.summary.as_ref().map(Message::encode_to_vec),
            queue: projection.queue.as_ref().map(Message::encode_to_vec),
        };
        if cached.items.iter().map(Vec::len).sum::<usize>() > 8 * 1024 * 1024 {
            return Ok(());
        }
        self.update_record(|r| {
            if r.chats.len() >= 8
                && !r.chats.contains_key(chat)
                && let Some(key) = r.chats.keys().next().cloned()
            {
                r.chats.remove(&key);
            }
            r.chats.insert(chat.into(), cached);
            while r
                .chats
                .values()
                .map(|c| c.items.iter().map(Vec::len).sum::<usize>())
                .sum::<usize>()
                > 8 * 1024 * 1024
            {
                let key = r.chats.keys().find(|key| key.as_str() != chat).cloned();
                if let Some(key) = key {
                    r.chats.remove(&key);
                } else {
                    break;
                }
            }
            Ok(())
        })
    }
    pub fn next_view_revision() -> u64 {
        static REVISION: AtomicU64 = AtomicU64::new(1);
        REVISION.fetch_add(1, Ordering::Relaxed)
    }
    pub fn save_view_version(
        &self,
        key: String,
        value: String,
        revision: u64,
    ) -> Result<(), String> {
        if value.len() > 1024 * 1024 {
            return Err("Draft exceeds the local 1 MiB limit.".into());
        }
        let mut state = self.state.lock().unwrap();
        if state
            .view_revisions
            .get(&key)
            .is_some_and(|saved| *saved > revision)
        {
            return Ok(());
        }
        let mut record = state.record.clone();
        if record.views.len() >= 100 && !record.views.contains_key(&key) {
            return Err("Local draft limit reached.".into());
        }
        record.views.insert(key.clone(), value);
        self.store.lock().unwrap().save(record.clone())?;
        state.record = record;
        state.view_revisions.insert(key, revision);
        Ok(())
    }
    pub fn save_view(&self, key: String, value: String) -> Result<(), String> {
        self.save_view_version(key, value, Self::next_view_revision())
    }
}
pub fn rpc_error(error: tonic::Status) -> String {
    format!("{}: {}", error.code(), error.message())
}

impl Drop for Registry {
    fn drop(&mut self) {
        if let Ok(hosts) = self.hosts.lock() {
            for host in hosts.values() {
                host.disconnect();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) struct FakeVault;
    impl CredentialStore for FakeVault {
        fn load(&self, _: &str) -> Result<String, String> {
            Err("not provisioned".into())
        }
        fn save(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    pub(super) fn record() -> HostRecord {
        HostRecord {
            id: HostId::parse(&uuid::Uuid::new_v4().to_string()).unwrap(),
            name: "Host".into(),
            endpoint: "https://127.0.0.1:1".into(),
            server_name: "test.local".into(),
            certificate: vec![],
            fingerprint: vec![],
            device_id: "device".into(),
            credential_ref: "fake".into(),
            ssh: None,
            routes: vec![],
            last_seen: None,
            epoch: Some(1),
            closing_epoch: None,
            catalog: vec![],
            outbox: vec![],
            mutations: vec![],
            views: BTreeMap::new(),
            uploads: vec![],
            chats: BTreeMap::new(),
        }
    }
    #[tokio::test]
    async fn lost_mutation_result_survives_restore_and_does_not_repeat_effect() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("client.json");
        let registry = ClientRegistry::open(path.clone(), Arc::new(FakeVault)).unwrap();
        let host = registry.insert(record()).unwrap();
        host.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("https://127.0.0.1:1").connect_lazy());
        let effects = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = effects.clone();
        assert!(
            host.mutate(None, move |_, _| async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(tonic::Status::unavailable("ack lost"))
            })
            .await
            .is_err()
        );
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(FakeVault))
            .unwrap()
            .get(&id)
            .unwrap();
        restored.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("https://127.0.0.1:1").connect_lazy());
        let count = effects.clone();
        assert!(
            restored
                .mutate(None, move |_, _| async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(tonic::Response::new(p::CommandReceipt::default()))
                })
                .await
                .is_err()
        );
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        assert_eq!(restored.recovery().len(), 1);
    }
    #[tokio::test]
    async fn journal_failure_prevents_mutation_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("client.json");
        let registry = ClientRegistry::open(path.clone(), Arc::new(FakeVault)).unwrap();
        let host = registry.insert(record()).unwrap();
        host.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("https://127.0.0.1:1").connect_lazy());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let effects = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = effects.clone();
        assert!(
            host.mutate(None, move |_, _| async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(tonic::Response::new(p::CommandReceipt::default()))
            })
            .await
            .is_err()
        );
        assert_eq!(effects.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn chat_intent_survives_lost_transport_and_blocks_a_new_command() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("client.json");
        let registry = ClientRegistry::open(path.clone(), Arc::new(FakeVault)).unwrap();
        let host = registry.insert(record()).unwrap();
        host.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy());
        let chat = p::ChatRef {
            host_id: host.id().as_str().into(),
            chat_id: "chat".into(),
        };
        let intent = ChatMutation::RunShell(p::RunShellRequest {
            chat: Some(chat.clone()),
            run_id: "run".into(),
            command_line: "echo preserve-intent".into(),
            ..Default::default()
        });
        assert!(host.submit_chat_mutation(intent.clone()).await.is_err());
        let pending = host.chat_mutations(&chat);
        assert_eq!(pending.len(), 1);
        let original = pending[0].0.clone();
        assert!(
            host.submit_chat_mutation(intent)
                .await
                .unwrap_err()
                .contains("unresolved")
        );
        assert_eq!(host.chat_mutations(&chat)[0].0, original);
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(FakeVault))
            .unwrap()
            .get(&id)
            .unwrap();
        let entries = restored.chat_mutations(&chat);
        assert_eq!(entries[0].0, original);
        let ChatMutation::RunShell(request) = &entries[0].1 else {
            panic!("wrong recovered operation")
        };
        assert_eq!(request.command_line, "echo preserve-intent");
        assert_eq!(request.command.as_ref().unwrap().command_id, original);
    }
    #[test]
    fn delayed_draft_worker_cannot_overwrite_a_newer_edit() {
        let directory = tempfile::tempdir().unwrap();
        let registry =
            ClientRegistry::open(directory.path().join("client.json"), Arc::new(FakeVault))
                .unwrap();
        let host = registry.insert(record()).unwrap();
        host.save_view_version("chat".into(), "new".into(), 2)
            .unwrap();
        host.save_view_version("chat".into(), "old".into(), 1)
            .unwrap();
        assert_eq!(host.snapshot().record.view("chat"), Some("new"));
    }
    #[test]
    fn every_window_receives_coalesced_host_changes() {
        let directory = tempfile::tempdir().unwrap();
        let registry =
            ClientRegistry::open(directory.path().join("client.json"), Arc::new(FakeVault))
                .unwrap();
        let host = registry.insert(record()).unwrap();
        let first = host.events();
        let second = host.events();
        host.changed();
        host.changed();
        first.try_recv().unwrap();
        second.try_recv().unwrap();
        assert!(first.try_recv().is_err());
        drop(first);
        host.changed();
        second.try_recv().unwrap();
        assert_eq!(host.listeners.lock().unwrap().len(), 1);
    }
}
