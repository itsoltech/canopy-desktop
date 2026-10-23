//! A removed conversation retains its resources and one joinable, retryable cleanup.
use super::*;
use canopy_desktop::state::workspace::Pane;
use std::path::Path;

impl Chats {
    pub fn is_opening(&self) -> bool {
        self.opening.is_some()
    }

    pub fn directory_reserved(&self, path: &Path) -> bool {
        self.retired_folders.iter().any(|cwd| path.starts_with(cwd))
    }

    /// A model check, without touching the filesystem or starting dormant chats.
    pub fn deletion_blocker(&self, ids: &[PaneId], cx: &App) -> Option<String> {
        let app = cx.global::<AppState>();
        let scratch = app.projects.read(cx).scratch_workspace(cx);
        scratch
            .all_panes()
            .iter()
            .filter(|p| ids.contains(&p.id) && p.metadata.kind == PaneKind::Chat)
            .find_map(|p| self.directory_blocker(p.metadata.cwd.as_deref()?, ids, cx))
    }

    fn directory_blocker(&self, dir: &Path, excluding: &[PaneId], cx: &App) -> Option<String> {
        let app = cx.global::<AppState>();
        let projects = app.projects.read(cx);
        for workspace in projects.runtime_workspaces(cx) {
            for tab in workspace.tabs() {
                for p in tab.root.panes() {
                    if !excluding.contains(&p.id) && uses_directory(&p, dir) {
                        return Some(format!(
                            "Close “{}” before deleting this chat; it uses the chat's folder.",
                            p.metadata.title.as_deref().unwrap_or(&tab.title)
                        ));
                    }
                }
            }
        }
        if let Some(project) = projects
            .catalog
            .items
            .iter()
            .find(|p| p.path.starts_with(dir))
        {
            return Some(format!(
                "Close project “{}” before deleting this chat; it uses the chat's folder.",
                project.name
            ));
        }
        if app.terminals.read(cx).uses_directory(dir, cx) {
            return Some(
                "A terminal still uses this chat's folder. Stop it and retry cleanup.".into(),
            );
        }
        if self.retiring.iter().any(|r| {
            !excluding.contains(&r.pane.id)
                && uses_directory(&r.pane, dir)
                && !r
                    .result
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(Result::is_ok)
        }) {
            return Some(
                "Another chat is still releasing this folder. Retry cleanup after it finishes."
                    .into(),
            );
        }
        None
    }

    pub fn cleanup_errors(&self) -> Vec<(PaneId, String)> {
        self.retiring
            .iter()
            .filter_map(|r| {
                let result = r.result.lock().unwrap_or_else(|e| e.into_inner());
                result.as_ref()?.as_ref().err().map(|e| {
                    (
                        r.pane.id,
                        format!(
                            "Could not delete “{}”: {e}",
                            r.pane.metadata.title.as_deref().unwrap_or("Chat")
                        ),
                    )
                })
            })
            .collect()
    }

    pub fn retry_cleanup(&mut self, id: PaneId, cx: &mut Context<Self>) {
        let Some(at) = self.retiring.iter().position(|r| {
            r.pane.id == id
                && r.result
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(Result::is_err)
        }) else {
            return;
        };
        let old = self.retiring.remove(at);
        self.retire(old.workspace, old.pane, old.view, cx);
        cx.notify();
    }

    pub(super) fn retire(
        &mut self,
        workspace: WorkspaceId,
        pane: Pane,
        view: Option<Entity<ChatView>>,
        cx: &mut Context<Self>,
    ) {
        if self.retiring.iter().any(|r| r.pane.id == pane.id) {
            return;
        }
        if !cx.has_global::<ChatFolders>() {
            cx.set_global(ChatFolders::default());
        }
        let standalone = workspace == cx.global::<AppState>().projects.read(cx).scratch_id(cx);
        let folder = pane.metadata.cwd.clone().filter(|dir| {
            standalone
                || dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
                    && dir
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|s| s == "chats")
        });
        if let Some(cwd) = &folder {
            self.retired_folders.insert(cwd.clone());
            cx.global_mut::<ChatFolders>()
                .0
                .insert(pane.id, cwd.clone());
        }
        let history = cx.global::<AppState>().chat_history.clone();
        let chat = pane
            .metadata
            .chat
            .as_ref()
            .and_then(|s| s.id.clone())
            .or_else(|| view.as_ref().and_then(|v| v.read(cx).chat_id()));
        if let Some(chat) = &chat {
            history.update(cx, |h, _| h.mark_deleted(chat));
        }
        let close = view.as_ref().map(|v| v.update(cx, |v, cx| v.close(cx)));
        let (finished, completion) = async_channel::bounded::<()>(1);
        let result = Arc::new(std::sync::Mutex::new(None));
        let shared = result.clone();
        let cwd = pane.metadata.cwd.clone();
        let id = pane.id;
        let watch = cx.spawn(async move |this, cx| {
            let outcome = async {
                if let Some(close) = close {
                    close.await?;
                }
                if let Some(cwd) = cwd {
                    let check = cwd.clone();
                    let data = cx
                        .background_executor()
                        .spawn(async move {
                            let data = canopy_desktop::platform::directories::data_dir()?;
                            Ok::<_, String>(
                                canopy_desktop::chat::scratch::owns(&data, &check).then_some(data),
                            )
                        })
                        .await?;
                    if data.is_none() {
                        let _ = this.update(cx, |this, cx| {
                            this.retired_folders.remove(&cwd);
                            cx.global_mut::<ChatFolders>().0.remove(&id);
                        });
                    }
                    if let Some(data) = data {
                        let blocker = this
                            .read_with(cx, |this, cx| this.directory_blocker(&cwd, &[id], cx))
                            .map_err(|e| e.to_string())?;
                        if let Some(error) = blocker {
                            return Err(error);
                        }
                        let references = this
                            .read_with(cx, |this, cx| this.folder_references(id, cx))
                            .map_err(|e| e.to_string())?;
                        cx.background_executor()
                            .spawn(async move {
                                check_references(&cwd, &references)?;
                                canopy_desktop::chat::scratch::remove(&data, &cwd)?;
                                Ok::<_, String>(())
                            })
                            .await?;
                    }
                }
                if let Some(chat) = chat {
                    let delete = history.update(cx, |h, cx| h.delete(&chat, cx));
                    delete.await?;
                }
                Ok(())
            }
            .await;
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            drop(finished);
            // A late file-open result must not recreate this removed UUID folder.
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        self.retiring.push(Retiring {
            workspace,
            pane,
            folder,
            view,
            completion,
            result,
            _watch: watch,
        });
    }

    pub(super) fn pending_cleanup(
        &self,
        workspace: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) -> Vec<Task<Result<(), String>>> {
        self.retiring
            .iter()
            .filter(|r| workspace.is_none_or(|w| r.workspace == w))
            .map(|r| {
                let completion = r.completion.clone();
                let result = r.result.clone();
                cx.spawn(async move |_, _| {
                    let _ = completion.recv().await;
                    result
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone()
                        .unwrap_or_else(|| Err("Chat cleanup result was lost.".into()))
                })
            })
            .collect()
    }

    fn folder_references(&self, excluding: PaneId, cx: &App) -> Vec<(std::path::PathBuf, String)> {
        let app = cx.global::<AppState>();
        let projects = app.projects.read(cx);
        let mut references = vec![];
        for workspace in projects.runtime_workspaces(cx) {
            for tab in workspace.tabs() {
                for pane in tab.root.panes().into_iter().filter(|p| p.id != excluding) {
                    if let Some(cwd) = &pane.metadata.cwd {
                        references.push((cwd.clone(), tab.title.clone()));
                    }
                    if matches!(
                        pane.metadata.kind,
                        PaneKind::Editor | PaneKind::Image | PaneKind::Font | PaneKind::Video
                    ) && let Some(resource) = &pane.metadata.resource
                    {
                        let path = std::path::PathBuf::from(resource);
                        let path = if path.is_absolute() {
                            Some(path)
                        } else {
                            pane.metadata.cwd.as_ref().map(|cwd| cwd.join(path))
                        };
                        if let Some(path) = path {
                            references.push((path, tab.title.clone()));
                        }
                    }
                }
            }
        }
        references.extend(
            projects
                .catalog
                .items
                .iter()
                .map(|p| (p.path.clone(), p.name.clone())),
        );
        references.extend(
            app.terminals
                .read(cx)
                .folder_references(cx)
                .into_iter()
                .map(|p| (p, "Terminal".into())),
        );
        references.extend(
            self.retiring
                .iter()
                .filter(|r| r.pane.id != excluding)
                .filter_map(|r| r.folder.clone().map(|p| (p, "Closing chat".into()))),
        );
        references
    }
}

/// Run on a worker immediately before deletion, after the model-side reservation.
fn check_references(dir: &Path, references: &[(std::path::PathBuf, String)]) -> Result<(), String> {
    let canonical = match std::fs::canonicalize(dir) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not confirm the chat folder: {error}")),
    };
    for (path, label) in references {
        match std::fs::canonicalize(path) {
            Ok(path) if path.starts_with(&canonical) => {
                return Err(format!(
                    "Close “{label}” before deleting this chat; it uses the chat's folder."
                ));
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!(
                    "Could not confirm the folder used by “{label}”: {error}"
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn uses_directory(pane: &Pane, dir: &Path) -> bool {
    if pane
        .metadata
        .cwd
        .as_deref()
        .is_some_and(|cwd| cwd.starts_with(dir))
    {
        return true;
    }
    matches!(
        pane.metadata.kind,
        PaneKind::Editor | PaneKind::Image | PaneKind::Font | PaneKind::Video
    ) && pane
        .metadata
        .resource
        .as_deref()
        .is_some_and(|file| Path::new(file).is_absolute() && Path::new(file).starts_with(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::settings::{Access, ChatHistoryBatch, SettingsClient};
    use canopy_desktop::state::workspace::Workspace;
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;

    #[cfg(unix)]
    #[test]
    fn an_open_file_through_an_alias_protects_the_private_folder() {
        let folder = tempfile::tempdir().unwrap();
        let private = folder.path().join("private");
        std::fs::create_dir(&private).unwrap();
        std::fs::write(private.join("note"), "keep").unwrap();
        let alias = folder.path().join("alias");
        std::os::unix::fs::symlink(&private, &alias).unwrap();
        let error =
            check_references(&private, &[(alias.join("note"), "Open note".into())]).unwrap_err();
        assert!(error.contains("Open note"));
        assert_eq!(
            std::fs::read_to_string(private.join("note")).unwrap(),
            "keep"
        );
    }

    #[gpui_kit::test]
    async fn deletion_without_a_view_retains_failure_and_quit_joins_its_retry(
        cx: &mut TestAppContext,
    ) {
        cx.background_executor.allow_parking();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let chat = ChatSettings::new_id();
        client
            .save_chat_history(vec![ChatHistoryBatch {
                chat: chat.clone(),
                epochs: vec![(0, "{}".into())],
                items: vec![(0, 1, "{}".into())],
            }])
            .await
            .unwrap();
        let readonly = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        let (app, pane, pending) = cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            app.chat_history
                .update(cx, |h, cx| h.initialize(readonly.clone(), cx));
            let pane = Pane {
                id: PaneId::new(),
                tool: "codex".into(),
                metadata: PaneMetadata {
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(ChatSettings {
                        id: Some(chat.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            };
            let id = pane.id;
            let pending = app.chats.update(cx, |chats, cx| {
                chats.retire(app.workspace.read(cx).id, pane, None, cx);
                chats.pending_cleanup(None, cx).pop().unwrap()
            });
            (app, id, pending)
        });
        assert!(
            pending.await.is_err(),
            "readonly SQL must keep a retryable cleanup"
        );
        assert_eq!(
            client
                .load_chat_history(chat.clone())
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        let quit = cx.update(|cx| {
            assert!(app.chats.read(cx).views.is_empty());
            assert_eq!(app.chats.read(cx).cleanup_errors()[0].0, pane);
            app.chat_history
                .update(cx, |h, cx| h.initialize(client.clone(), cx));
            app.chats.update(cx, |chats, cx| chats.shutdown(cx))
        });
        quit.await.unwrap();
        assert!(
            client
                .load_chat_history(chat)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        cx.update(|cx| assert!(app.chats.read(cx).retiring.is_empty()));
        readonly.shutdown().await.unwrap();
        client.shutdown().await.unwrap();
    }
}
