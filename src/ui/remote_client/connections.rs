//! Shared saved connections for Preferences and client windows. Loading uses the
//! client's existing OnceCell/store; opening Preferences never connects a host.
use super::*;

#[derive(Default)]
pub struct Connections {
    registry: Option<ClientRegistry>,
    loading: bool,
    error: Option<String>,
    watches: BTreeMap<String, Task<()>>,
    registry_watch: Option<Task<()>>,
}
impl Connections {
    pub fn hosts(&self) -> Vec<Arc<HostClient>> {
        self.registry
            .as_ref()
            .map(ClientRegistry::hosts)
            .unwrap_or_default()
    }
    pub fn loading(&self) -> bool {
        self.loading
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn load(&mut self, global: ClientGlobal, cx: &mut Context<Self>) {
        if self.registry.is_some() || self.loading {
            return;
        }
        self.loading = true;
        let task = global
            .runtime
            .clone()
            .spawn(async move { global.load_registry().await });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|_| Err("Could not load saved connections.".into()));
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(registry) => this.install(registry, cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn install(&mut self, registry: ClientRegistry, cx: &mut Context<Self>) {
        self.error = None;
        if self.registry_watch.is_none() {
            let events = registry.events();
            let registry = registry.clone();
            self.registry_watch = Some(cx.spawn(async move |this, cx| {
                while events.recv().await.is_ok() {
                    if this
                        .update(cx, |this, cx| this.install(registry.clone(), cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        for host in registry.hosts() {
            let id = host.id().as_str().to_owned();
            if self.watches.contains_key(&id) {
                continue;
            }
            let events = host.events();
            let task = cx.spawn(async move |this, cx| {
                while events.recv().await.is_ok() {
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            });
            self.watches.insert(id, task);
        }
        self.registry = Some(registry);
        cx.notify();
    }
}
pub fn connections(cx: &mut App) -> Result<Entity<Connections>> {
    ensure_global(cx)?;
    let global = cx.global::<ClientGlobal>().clone();
    let view = global.connections.clone();
    view.update(cx, |view, cx| view.load(global, cx));
    Ok(view)
}
