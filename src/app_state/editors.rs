use super::AppState;
use crate::ui::editor::{EditorChanged, EditorView};
use canopy_desktop::state::workspace::{PaneId, PaneKind};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
pub struct Editors {
    pub views: HashMap<PaneId, Entity<EditorView>>,
    pub fonts: HashMap<PaneId, Entity<crate::ui::font_preview::FontPreview>>,
    #[cfg(target_os = "macos")]
    pub videos: HashMap<PaneId, Entity<crate::ui::video_preview::VideoPreview>>,
    events: HashMap<PaneId, Subscription>,
    pub images: HashMap<PaneId, Entity<crate::ui::image_preview::ImagePreview>>,
    paused: HashSet<PaneId>,
}
impl Editors {
    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
            fonts: HashMap::new(),
            #[cfg(target_os = "macos")]
            videos: HashMap::new(),
            events: HashMap::new(),
            images: HashMap::new(),
            paused: HashSet::new(),
        }
    }
    pub fn busy(&self, ids: &[PaneId], cx: &App) -> bool {
        ids.iter().any(|id| {
            self.views
                .get(id)
                .is_some_and(|v| v.read(cx).dirty || v.read(cx).saving)
        })
    }
    pub fn any_dirty(&self, cx: &App) -> bool {
        self.views
            .values()
            .any(|v| v.read(cx).dirty || v.read(cx).saving)
    }
    pub fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        if !app.projects.read(cx).ready {
            return;
        }
        let alive: HashSet<_> = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        self.fonts.retain(|id, view| {
            if alive.contains(id) {
                true
            } else {
                view.update(cx, |view, cx| view.release(cx));
                false
            }
        });
        #[cfg(target_os = "macos")]
        self.videos.retain(|id, view| {
            if alive.contains(id) {
                true
            } else {
                view.update(cx, |view, _| view.release());
                false
            }
        });
        self.images.retain(|id, view| {
            if alive.contains(id) {
                true
            } else {
                view.update(cx, |view, cx| view.release(cx));
                false
            }
        });
        self.views.retain(|id, _| alive.contains(id));
        self.events.retain(|id, _| alive.contains(id));
        let workspace = app.workspace.read(cx);
        let active = workspace.activation_plan();
        let focused = workspace.active().map(|t| t.focused);
        for pane in active.iter().filter(|p| {
            !self.paused.contains(&p.id)
                && matches!(
                    p.metadata.kind,
                    PaneKind::Editor | PaneKind::Image | PaneKind::Font | PaneKind::Video
                )
        }) {
            let path = std::path::Path::new(pane.metadata.resource.as_deref().unwrap_or(""));
            if pane.metadata.kind == PaneKind::Font || canopy_desktop::files::is_font(path) {
                self.fonts.entry(pane.id).or_insert_with(|| {
                    cx.new(|cx| crate::ui::font_preview::FontPreview::new(pane.clone(), cx))
                });
                continue;
            }
            if pane.metadata.kind == PaneKind::Video || canopy_desktop::files::is_video(path) {
                #[cfg(target_os = "macos")]
                self.videos.entry(pane.id).or_insert_with(|| {
                    cx.new(|cx| {
                        crate::ui::video_preview::VideoPreview::new(pane.clone(), window, cx)
                    })
                });
                continue;
            }
            if pane.metadata.kind == PaneKind::Image
                || pane
                    .metadata
                    .resource
                    .as_deref()
                    .is_some_and(|p| canopy_desktop::files::is_image(std::path::Path::new(p)))
            {
                self.images.entry(pane.id).or_insert_with(|| {
                    cx.new(|cx| crate::ui::image_preview::ImagePreview::new(pane.clone(), cx))
                });
                continue;
            }
            if !self.views.contains_key(&pane.id) {
                let view = cx.new(|cx| EditorView::new(pane.clone(), window, cx));
                self.events.insert(
                    pane.id,
                    cx.subscribe(&view, |_, _, _: &EditorChanged, cx| cx.notify()),
                );
                self.views.insert(pane.id, view);
            }
        }
        #[cfg(target_os = "macos")]
        for (id, view) in &self.videos {
            view.update(cx, |view, cx| {
                view.visibility(active.iter().any(|p| p.id == *id), false, cx)
            });
        }
        for (id, view) in &self.views {
            if self.paused.contains(id) {
                continue;
            }
            view.update(cx, |view, cx| {
                view.resume_filesystem_handles(window, cx);
                view.activate(active.iter().any(|p| p.id == *id), focused == Some(*id), cx)
            });
        }
        cx.notify();
    }

    pub fn pause_for_cleanup(&mut self, ids: &[PaneId], cx: &mut Context<Self>) {
        self.paused.extend(ids.iter().copied());
        for id in ids {
            if let Some(view) = self.views.get(id) {
                view.update(cx, |view, _| view.release_filesystem_handles());
            }
            if let Some(view) = self.images.remove(id) {
                view.update(cx, |view, cx| view.release(cx));
            }
            if let Some(view) = self.fonts.remove(id) {
                view.update(cx, |view, cx| view.release(cx));
            }
            #[cfg(target_os = "macos")]
            if let Some(view) = self.videos.remove(id) {
                view.update(cx, |view, _| view.release());
            }
        }
        cx.notify();
    }

    pub fn resume_after_cleanup(&mut self, ids: &[PaneId], cx: &mut Context<Self>) {
        for id in ids {
            self.paused.remove(id);
        }
        let workspace = cx.global::<AppState>().workspace.clone();
        workspace.update(cx, |_, cx| cx.notify());
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::{app_state::AppState, ui::theme};
    use canopy_desktop::state::workspace::{PaneKind, PaneMetadata, Workspace};
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};
    use std::{cell::Cell, rc::Rc};

    struct EditorHarness {
        editor: Entity<EditorView>,
        editors: Entity<Editors>,
        notifications: Rc<Cell<usize>>,
        _observer: Subscription,
    }

    impl Render for EditorHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.editor.clone()
        }
    }

    #[gpui_kit::test]
    fn pause_resume_keeps_editor_change_subscription(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.txt"), "initial").unwrap();
        let (window, pane) = cx.update(|cx| {
            gpui_kit::init(cx);
            theme::init(cx);
            let mut workspace = Workspace::new();
            let tab = workspace.open("note.txt", "editor");
            let pane = workspace.active().unwrap().focused;
            workspace
                .set_pane_metadata(
                    tab,
                    pane,
                    PaneMetadata {
                        cwd: Some(directory.path().to_owned()),
                        resource: Some("note.txt".into()),
                        kind: PaneKind::Editor,
                        ..Default::default()
                    },
                )
                .unwrap();
            let pane_model = workspace.active().unwrap().root.find(pane).unwrap().clone();
            let app = AppState::install_test(workspace, cx);
            let editors = app.editors.clone();
            let window = cx
                .open_window(Default::default(), |window, cx| {
                    let editor = cx.new(|cx| EditorView::new(pane_model, window, cx));
                    editors.update(cx, |state, cx| {
                        state.events.insert(
                            pane,
                            cx.subscribe(&editor, |_, _, _: &EditorChanged, cx| cx.notify()),
                        );
                        state.views.insert(pane, editor.clone());
                    });
                    let notifications = Rc::new(Cell::new(0));
                    let count = notifications.clone();
                    cx.new(|cx| EditorHarness {
                        editor,
                        editors: editors.clone(),
                        notifications,
                        _observer: cx.observe(&editors, move |_, _, _| count.set(count.get() + 1)),
                    })
                })
                .unwrap();
            (window, pane)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let harness = window.root(&mut cx).unwrap();
        cx.run_until_parked();

        let editors = harness.read_with(&cx, |harness, _| harness.editors.clone());
        cx.update(|_, cx| {
            editors.update(cx, |editors, cx| {
                editors.pause_for_cleanup(&[pane], cx);
                editors.resume_after_cleanup(&[pane], cx);
                assert!(editors.events.contains_key(&pane));
            });
        });
        let before = harness.read_with(&cx, |harness, _| harness.notifications.get());
        let focus = harness.read_with(&cx, |harness, cx| {
            harness.editor.read(cx).input_focus_handle(cx)
        });
        cx.update(|window, cx| focus.focus(window, cx));
        cx.simulate_input("x");

        harness.read_with(&cx, |harness, cx| {
            assert!(harness.editor.read(cx).dirty);
            assert!(harness.editors.read(cx).busy(&[pane], cx));
            assert!(harness.notifications.get() > before);
        });
    }
}

pub struct GuardRequest {
    pub ids: Vec<PaneId>,
    pub action: std::rc::Rc<dyn Fn(&mut App)>,
}
impl EventEmitter<GuardRequest> for Editors {}
impl Editors {
    pub fn guard(ids: Vec<PaneId>, action: impl Fn(&mut App) + 'static, cx: &mut App) {
        let editors = cx.global::<AppState>().editors.clone();
        if editors.read(cx).busy(&ids, cx) {
            editors.update(cx, |_, cx| {
                cx.emit(GuardRequest {
                    ids,
                    action: std::rc::Rc::new(action),
                })
            });
        } else {
            action(cx);
        }
    }
    pub fn confirm(request: &GuardRequest, window: &mut Window, cx: &mut App) {
        if window.has_active_prompt() {
            return;
        }
        let ids = request.ids.clone();
        let action = request.action.clone();
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes before closing?",
            Some("Your unsaved edits will be lost if you discard them."),
            &[
                PromptButton::cancel("Cancel"),
                PromptButton::ok("Save and close"),
                PromptButton::new("Discard and close"),
            ],
            cx,
        );
        let editors = cx.global::<AppState>().editors.clone();
        window
            .spawn(cx, async move |cx| {
                let Ok(answer) = answer.await else {
                    return;
                };
                if answer == 0 {
                    return;
                }
                let (tx, rx) = async_channel::bounded(1);
                let observer = cx
                    .update(|_, cx| {
                        cx.observe(&editors, move |_, _| {
                            let _ = tx.try_send(());
                        })
                    })
                    .ok();
                let _ = cx.update(|_, cx| {
                    for id in &ids {
                        if let Some(view) = editors.read(cx).views.get(id).cloned() {
                            view.update(cx, |view, cx| {
                                if answer == 1 {
                                    view.save(cx);
                                } else if !view.saving {
                                    view.dirty = false;
                                }
                            });
                        }
                    }
                });
                loop {
                    let state = cx.update(|_, cx| {
                        let editors = editors.read(cx);
                        (
                            ids.iter()
                                .any(|id| editors.views.get(id).is_some_and(|v| v.read(cx).saving)),
                            editors.busy(&ids, cx),
                        )
                    });
                    match state {
                        Ok((true, _)) => {
                            if rx.recv().await.is_err() {
                                break;
                            }
                        }
                        Ok((false, false)) => {
                            let _ = cx.update(|_, cx| action(cx));
                            break;
                        }
                        _ => break,
                    }
                }
                drop(observer);
            })
            .detach();
    }
}

impl Editors {
    pub fn media_view(&self, pane: PaneId) -> Option<AnyView> {
        if let Some(view) = self.fonts.get(&pane) {
            return Some(view.clone().into());
        }
        #[cfg(target_os = "macos")]
        if let Some(view) = self.videos.get(&pane) {
            return Some(view.clone().into());
        }
        None
    }
    pub fn obscure_media(&self, hidden: bool, cx: &mut App) {
        #[cfg(target_os = "macos")]
        {
            let active = cx.global::<AppState>().workspace.read(cx).activation_plan();
            for (id, view) in &self.videos {
                view.update(cx, |view, cx| {
                    view.visibility(active.iter().any(|p| p.id == *id), hidden, cx)
                });
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (hidden, cx);
    }
}
