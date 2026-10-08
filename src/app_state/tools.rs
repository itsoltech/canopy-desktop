//! Shared, committed tool catalog and executable discovery.
use super::AppState;
use canopy_desktop::{
    settings::SettingsClient,
    state::tools::{ToolCatalog, ToolKind},
    terminal::environment::ShellEnvironment,
};
use gpui_kit::*;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
#[derive(Clone)]
pub struct WorktreeAgent {
    pub tool: String,
    pub profile: Option<String>,
}
pub struct ToolsState {
    pub catalog: ToolCatalog,
    pub ready: bool,
    pub saving: bool,
    pub error: Option<String>,
    pub environment: Option<Result<Arc<ShellEnvironment>, String>>,
    pub availability: HashMap<String, Result<PathBuf, String>>,
    client: Option<SettingsClient>,
    task: Option<Task<()>>,
    probe: Option<Task<()>>,
    pub discovering: bool,
    refresh_pending: bool,
    pub(super) quitting: bool,
}

async fn record_failed_cleanup(
    mut recovery: ToolCatalog,
    failed: Vec<String>,
    client: &SettingsClient,
) -> (ToolCatalog, String) {
    if failed.is_empty() {
        return (recovery, String::new());
    }
    for id in failed {
        if !recovery.retired_credentials.contains(&id) {
            recovery.retired_credentials.push(id);
        }
    }
    let warning = if client.save_tools(recovery.clone()).await.is_ok() {
        " Cleanup of a newly written API key failed and will be retried on the next save or start."
    } else {
        " Cleanup of a newly written API key failed, and its retry state could not be saved. Keep Canopy open and save Tools again to retry."
    };
    (recovery, warning.into())
}

impl ToolsState {
    pub fn new() -> Self {
        Self {
            catalog: ToolCatalog::default(),
            ready: false,
            saving: false,
            error: None,
            environment: None,
            availability: HashMap::new(),
            client: None,
            task: None,
            probe: None,
            discovering: false,
            refresh_pending: false,
            quitting: false,
        }
    }
    pub fn initialize(&mut self, client: SettingsClient, reopen: bool, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        self.client = Some(client.clone());
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let catalog = client
                    .load_tools()
                    .await
                    .map_err(|error| error.to_string())?;
                let retired = catalog.retired_credentials.clone();
                if retired.is_empty() {
                    return Ok((catalog, None));
                }
                let failed = cx
                    .background_executor()
                    .spawn(async move {
                        retired
                            .into_iter()
                            .filter(|id| canopy_desktop::terminal::credentials::remove(id).is_err())
                            .collect::<Vec<_>>()
                    })
                    .await;
                let (catalog, warning) = client.persist_tool_cleanup(catalog, failed).await;
                Ok::<_, String>((catalog, warning))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((catalog, warning)) => {
                        this.catalog = catalog;
                        this.ready = true;
                        this.error = warning;
                        this.refresh(cx);
                        cx.defer(move |cx| {
                            let projects = cx.global::<AppState>().projects.clone();
                            projects.update(cx, |state, cx| state.initialize(client, reopen, cx));
                        });
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        }));
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        if self.discovering {
            self.refresh_pending = true;
            return;
        }
        self.discovering = true;
        cx.notify();
        let catalog = self.catalog.clone();
        self.probe = Some(cx.spawn(async move |this, cx| {
            let (environment, availability) = cx
                .background_executor()
                .spawn(async move {
                    let environment = ShellEnvironment::load().map(Arc::new);
                    let availability = catalog
                        .tools
                        .iter()
                        .map(|t| {
                            let result =
                                environment.as_ref().map_err(Clone::clone).and_then(|env| {
                                    if t.kind == ToolKind::Shell && t.executable.is_empty() {
                                        Ok(env.shell.clone())
                                    } else {
                                        env.resolve(&t.executable)
                                    }
                                });
                            (t.id.clone(), result)
                        })
                        .collect();
                    (environment, availability)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.discovering = false;
                this.environment = Some(environment);
                this.availability = availability;
                if std::mem::take(&mut this.refresh_pending) {
                    this.refresh(cx);
                }
                cx.notify();
            });
        }));
    }
    pub fn save(&mut self, catalog: ToolCatalog, cx: &mut Context<Self>) -> Result<(), String> {
        self.save_with_keys(catalog, HashMap::new(), cx)
    }
    pub fn save_with_keys(
        &mut self,
        mut catalog: ToolCatalog,
        keys: HashMap<String, String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.ready || self.saving || self.quitting {
            return Err("Tools are busy. Try again shortly.".into());
        }
        for id in &self.catalog.retired_credentials {
            if !catalog.retired_credentials.contains(id) {
                catalog.retired_credentials.push(id.clone());
            }
        }
        catalog.validate()?;
        if keys
            .values()
            .any(|key| key.len() > 2_560 || key.contains('\0'))
        {
            return Err("API key is too long or contains invalid characters.".into());
        }
        let client = self.client.clone().ok_or("Tools storage is unavailable.")?;
        // New immutable references make a failed SQLite write leave the old key intact.
        let mut pending = Vec::new();
        for tool in &mut catalog.tools {
            for profile in &mut tool.profiles {
                if let Some(value) = keys.get(&profile.id) {
                    if value.is_empty() {
                        profile.settings.api_key_ref = None;
                    } else {
                        let id = canopy_desktop::state::tools::new_id();
                        profile.settings.api_key_ref = Some(id.clone());
                        pending.push((id, value.clone()));
                    }
                }
            }
        }
        let active_refs: std::collections::HashSet<_> = catalog
            .tools
            .iter()
            .flat_map(|tool| &tool.profiles)
            .filter_map(|profile| profile.settings.api_key_ref.as_deref())
            .collect();
        let old_refs: Vec<_> = self
            .catalog
            .tools
            .iter()
            .flat_map(|t| &t.profiles)
            .filter_map(|p| p.settings.api_key_ref.clone())
            .collect();
        for id in old_refs
            .into_iter()
            .filter(|id| !active_refs.contains(id.as_str()))
        {
            if !catalog.retired_credentials.contains(&id) {
                catalog.retired_credentials.push(id);
            }
        }
        catalog.validate()?;
        self.saving = true;
        self.error = None;
        cx.notify();
        let previous = self.catalog.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn(async move {
                    let mut written: Vec<String> = Vec::new();
                    for (id, value) in pending {
                        if let Err(error) =
                            canopy_desktop::terminal::credentials::store(&id, &value)
                        {
                            let cleanup_failed = written
                                .into_iter()
                                .filter(|id| {
                                    canopy_desktop::terminal::credentials::remove(id).is_err()
                                })
                                .collect();
                            return Err((error, cleanup_failed));
                        }
                        written.push(id);
                    }
                    Ok(written)
                })
                .await;
            let result = match written {
                Err((error, cleanup_failed)) => {
                    let (recovery, warning) =
                        record_failed_cleanup(previous, cleanup_failed, &client).await;
                    Err((format!("{error}{warning}"), recovery))
                }
                Ok(written) => {
                    let saved = client
                        .save_tools(catalog.clone())
                        .await
                        .map_err(|e| e.to_string());
                    if let Err(error) = saved {
                        let cleanup_failed = cx
                            .background_executor()
                            .spawn(async move {
                                written
                                    .into_iter()
                                    .filter(|id| {
                                        canopy_desktop::terminal::credentials::remove(id).is_err()
                                    })
                                    .collect()
                            })
                            .await;
                        let (recovery, warning) =
                            record_failed_cleanup(previous, cleanup_failed, &client).await;
                        Err((format!("{error}{warning}"), recovery))
                    } else {
                        let retired = catalog.retired_credentials.clone();
                        let failed = cx
                            .background_executor()
                            .spawn(async move {
                                retired
                                    .into_iter()
                                    .filter(|id| {
                                        canopy_desktop::terminal::credentials::remove(id).is_err()
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .await;
                        Ok(client.persist_tool_cleanup(catalog, failed).await)
                    }
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok((catalog, cleanup_warning)) => {
                        this.catalog = catalog;
                        this.refresh(cx);
                        this.error = cleanup_warning;
                    }
                    Err((error, recovery)) => {
                        this.catalog = recovery;
                        this.error = Some(format!("Could not save tools: {error}"));
                    }
                }
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn launch(&mut self, id: &str, profile: Option<&str>, cx: &mut Context<Self>) {
        let result = self.open(id, profile, None, false, cx);
        self.error = result.err();
        cx.notify();
    }
    pub fn validate_worktree_agent(&self, agent: &WorktreeAgent) -> Result<(), String> {
        if !self.ready || self.saving || self.quitting {
            return Err("Tools are not ready.".into());
        }
        let tool = self
            .catalog
            .get(&agent.tool)
            .ok_or("Agent no longer exists.")?;
        if !tool.enabled || !matches!(tool.id.as_str(), "claude" | "codex") {
            return Err("Agent is unavailable.".into());
        }
        if agent
            .profile
            .as_ref()
            .is_some_and(|id| !tool.profiles.iter().any(|p| &p.id == id))
        {
            return Err("Agent profile no longer exists.".into());
        }
        self.availability
            .get(&agent.tool)
            .ok_or("Wait for executable discovery.")?
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(())
    }
    pub fn launch_worktree_agent(
        &self,
        agent: &WorktreeAgent,
        path: &std::path::Path,
        prompt: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.validate_worktree_agent(agent)?;
        if cx
            .global::<AppState>()
            .projects
            .read(cx)
            .catalog
            .current()
            .is_none_or(|p| p.path != path)
        {
            return Err(
                "The created worktree is not active. Open it before starting the agent.".into(),
            );
        }
        self.open(&agent.tool, agent.profile.as_deref(), prompt, true, cx)
    }
    fn open(
        &self,
        id: &str,
        profile: Option<&str>,
        task_prompt: Option<String>,
        creating_worktree: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.ready || self.quitting {
            return Err("Tools are not ready.".into());
        }
        let tool = self.catalog.get(id).ok_or("Tool not found.")?;
        if !tool.enabled {
            return Err("Tool is disabled.".into());
        }
        let profile = profile
            .map(str::to_owned)
            .or_else(|| tool.default_profile.clone());
        if profile
            .as_ref()
            .is_some_and(|id| !tool.profiles.iter().any(|p| &p.id == id))
        {
            return Err("Profile not found.".into());
        }
        let app = cx.global::<AppState>().clone();
        if !creating_worktree && app.git.read(cx).busy {
            return Err("Wait for the Git operation to finish.".into());
        }
        let cwd = app
            .projects
            .read(cx)
            .catalog
            .current()
            .map(|p| p.worktree_path.clone().unwrap_or_else(|| p.path.clone()))
            .ok_or("Open a project first.")?;
        let title = tool.label(profile.as_deref());
        app.workspace.update(cx, |state, cx| {
            let tab = state.open(title.clone(), id);
            let pane = state.active().expect("new tab").focused;
            let metadata = canopy_desktop::state::workspace::PaneMetadata {
                cwd: Some(cwd),
                profile_id: profile,
                title: Some(title),
                task_prompt,
                ..Default::default()
            };
            let _ = state.set_pane_metadata(tab, pane, metadata);
            cx.notify();
        });
        Ok(())
    }
    pub fn shutdown_tasks(&mut self) -> Vec<Task<()>> {
        self.quitting = true;
        self.task
            .take()
            .into_iter()
            .chain(self.probe.take())
            .collect()
    }
}
