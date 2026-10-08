//! One document per pane; async reads, optimistic atomic saves and explicit conflicts.
use super::components::files::file_message;
use super::{components::*, theme as t};
use canopy_desktop::{
    files::{Document, Watch},
    state::workspace::Pane,
};
use gpui_kit::base::Disableable;
use gpui_kit::{
    component::input::{EditorState, InputEvent},
    *,
};
use std::{path::PathBuf, time::Duration};

actions!(file_editor, [SaveEditor]);

pub(crate) fn init(cx: &mut App) {
    init_for_platform(cx, cfg!(target_os = "windows"));
}

fn init_for_platform(cx: &mut App, windows: bool) {
    cx.bind_keys([KeyBinding::new(
        if windows { "ctrl-s" } else { "cmd-s" },
        SaveEditor,
        Some("FileEditor"),
    )]);
}

pub struct EditorChanged;
impl EventEmitter<EditorChanged> for EditorView {}
pub struct EditorView {
    pub root: PathBuf,
    pub path: PathBuf,
    input: Entity<EditorState>,
    _input_events: Subscription,
    document: Option<Document>,
    pub dirty: bool,
    pub saving: bool,
    error: Option<String>,
    watch_error: Option<String>,
    conflict: bool,
    loading: bool,
    watch: Option<Task<()>>,
    save_task: Option<Task<()>>,
    read_task: Option<Task<()>>,
    focus: bool,
    visible: bool,
    window: AnyWindowHandle,
    refresh_pending: bool,
}
fn language(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|v| v.to_str()).unwrap_or("") {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "json" => "json",
        "py" => "python",
        "html" | "vue" => "html",
        "svelte" => "svelte",
        "css" => "css",
        "md" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" => "bash",
        _ => "plain_text",
    }
}
impl EditorView {
    pub fn new(pane: Pane, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_watch(pane, true, window, cx)
    }

    fn new_with_watch(
        pane: Pane,
        watch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root = pane.metadata.cwd.unwrap_or_default();
        let path = PathBuf::from(pane.metadata.resource.unwrap_or_default());
        let input = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language(&path))
                .line_number(true)
        });
        let input_events = cx.subscribe(&input, |this, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let previous = this.dirty;
                this.dirty = this.document.as_ref().is_some_and(|d| {
                    d.text.len() != state.read(cx).text().len()
                        || d.text.as_str() != state.read(cx).value().as_str()
                });
                if this.dirty != previous {
                    cx.emit(EditorChanged);
                    cx.notify();
                }
            }
        });
        let mut this = Self {
            root,
            path,
            input,
            _input_events: input_events,
            document: None,
            dirty: false,
            saving: false,
            error: None,
            watch_error: None,
            conflict: false,
            loading: true,
            watch: None,
            save_task: None,
            read_task: None,
            focus: true,
            visible: false,
            window: window.window_handle(),
            refresh_pending: false,
        };
        this.reload(window, cx);
        if watch {
            this.start_watch(window, cx);
        }
        this
    }
    fn start_watch(&mut self, window: &Window, cx: &mut Context<Self>) {
        let path = self.root.join(&self.path);
        let parent = path.parent().unwrap_or(&self.root).to_owned();
        self.watch = Some(cx.spawn_in(window, async move |this, cx| {
            let watcher = cx
                .background_executor()
                .spawn(async move { Watch::new(&parent, false) })
                .await;
            let watcher = match watcher {
                Ok(v) => v,
                Err(_) => {
                    let _ = this.update(cx, |this, cx| {
                        this.watch_error = Some(
                            "File watching unavailable. Use Reload to check disk changes.".into(),
                        );
                        cx.notify();
                    });
                    return;
                }
            };
            // Register first, then re-read so no change can fall between load and watch.
            if update_editor(&this, cx, |this, window, cx| this.reload(window, cx)).is_err() {
                return;
            }
            while watcher.changes.recv().await.is_ok() {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                while watcher.changes.try_recv().is_ok() {}
                let watch_error = watcher.error();
                if update_editor(&this, cx, |this, window, cx| {
                    this.watch_error = watch_error;
                    if this.saving {
                        this.refresh_pending = true;
                    } else {
                        this.reload(window, cx);
                    }
                })
                .is_err()
                {
                    break;
                }
            }
        }));
    }
    pub fn activate(&mut self, visible: bool, focused: bool, cx: &mut Context<Self>) {
        if visible && (!self.visible || focused) {
            self.focus = focused;
            cx.notify();
        }
        self.visible = visible;
    }
    pub fn release_filesystem_handles(&mut self) {
        self.watch = None;
        self.read_task = None;
        if !self.saving {
            self.save_task = None;
        }
    }
    pub fn resume_filesystem_handles(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.watch.is_none() {
            self.start_watch(window, cx);
        }
    }
    #[cfg(test)]
    pub fn input_focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
    pub fn reload(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let (root, path) = (self.root.clone(), self.path.clone());
        self.read_task=Some(cx.spawn_in(window,async move |this,cx| {
            let result=cx.background_executor().spawn(async move {Document::load(&root,&path)}).await;
            let _=update_editor(&this,cx,|this,window,cx| {
                this.loading=false;
                match result {
                    Ok(document)=>{
                        if this.document.as_ref().is_some_and(|d|d.original==document.original) {
                            if this.conflict {this.conflict=false;this.error=None;cx.notify();}
                            return;
                        }
                        if this.dirty {this.conflict=true;this.error=Some("File changed on disk. Save is blocked until you reload or keep a copy of your edits.".into());}
                        else {this.input.update(cx,|input,cx|input.set_value(document.text.clone(),window,cx));this.document=Some(document);this.conflict=false;this.error=None;}
                    },
                    Err(error)=>{this.error=Some(error);this.conflict=this.document.is_some();}
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.loading || !self.dirty {
            return;
        }
        if self.conflict {
            self.error=Some("Resolve the disk change before saving. Reload discards your local edits after confirmation.".into());
            cx.notify();
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let (root, path, text) = (
            self.root.clone(),
            self.path.clone(),
            self.input.read(cx).value().to_string(),
        );
        self.saving = true;
        self.read_task = None;
        cx.emit(EditorChanged);
        cx.notify();
        self.save_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { document.save(&root, &path, &text) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(document) => {
                        this.dirty = this.input.read(cx).value().as_str() != document.text;
                        this.document = Some(document);
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                if this.refresh_pending {
                    this.refresh_pending = false;
                    let target = cx.entity().downgrade();
                    let window = this.window;
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| {
                            let _ = target.update(cx, |this, cx| this.reload(window, cx));
                        });
                    });
                }
                cx.emit(EditorChanged);
                cx.notify();
            });
        }));
    }
    pub(crate) fn request_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if !self.dirty {
            self.reload(window, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard your edits and reload this file?",
            None,
            &[
                PromptButton::cancel("Cancel"),
                PromptButton::new("Discard and reload"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = update_editor(&this, cx, |this, window, cx| {
                    this.dirty = false;
                    this.document = None;
                    this.loading = true;
                    this.reload(window, cx);
                    cx.emit(EditorChanged);
                });
            }
        })
        .detach();
    }
}
impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus && self.visible {
            self.focus = false;
            self.input.update(cx, |input, cx| input.focus(window, cx));
        }
        column()
            .key_context("FileEditor")
            .on_action(cx.listener(|this, _: &SaveEditor, _, cx| {
                this.save(cx);
                cx.stop_propagation();
            }))
            .size_full()
            .bg(t::bg())
            .children(
                self.error
                    .clone()
                    .or(self.watch_error.clone())
                    .map(|error| {
                        row()
                            .gap(px(8.))
                            .p(px(12.))
                            .child(file_message(error, t::red()).flex_1())
                            .child(
                                button("reload-file", "Reload from disk")
                                    .disabled(self.saving)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.request_reload(window, cx)
                                    })),
                            )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(if self.loading {
                        file_message("Loading file…", t::text())
                            .p(px(16.))
                            .into_any_element()
                    } else {
                        code_editor(&self.input)
                            .readonly(self.document.is_none())
                            .into_any_element()
                    }),
            )
    }
}

// Use the owning window, not the entity's last rendered window: hidden tabs still
// own live documents and must receive reads while their view is unmounted.
fn update_editor<R>(
    entity: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    update: impl FnOnce(&mut EditorView, &mut Window, &mut Context<EditorView>) -> R,
) -> Result<R> {
    cx.update(|window, cx| entity.update(cx, |view, cx| update(view, window, cx)))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::{
        state::workspace::{PaneKind, PaneMetadata, Workspace},
        terminal::environment::ShellEnvironment,
    };
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};
    use std::sync::Arc;

    struct Harness {
        first: Entity<EditorView>,
        second: Option<Entity<EditorView>>,
        terminal: Entity<super::super::terminal::TerminalView>,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            row()
                .size_full()
                .child(self.first.clone())
                .children(self.second.clone())
                .child(self.terminal.clone())
        }
    }

    fn pane(root: &std::path::Path, resource: &str) -> Pane {
        let mut workspace = Workspace::new();
        let tab = workspace.open(resource, "editor");
        let id = workspace.active().unwrap().focused;
        workspace
            .set_pane_metadata(
                tab,
                id,
                PaneMetadata {
                    cwd: Some(root.to_owned()),
                    resource: Some(resource.into()),
                    kind: PaneKind::Editor,
                    ..Default::default()
                },
            )
            .unwrap();
        workspace.active().unwrap().root.find(id).unwrap().clone()
    }

    fn open(
        root: &std::path::Path,
        second: Option<&str>,
        cx: &mut TestAppContext,
    ) -> (
        WindowHandle<Harness>,
        Entity<EditorView>,
        Option<Entity<EditorView>>,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::theme::init(cx);
            init_for_platform(cx, true);
            let first_pane = pane(root, "first.txt");
            let second_pane = second.map(|name| pane(root, name));
            let mut terminal_workspace = Workspace::new();
            terminal_workspace.open("Shell", "shell");
            let terminal_pane = terminal_workspace.activation_plan()[0].clone();
            let window = cx
                .open_window(Default::default(), move |window, cx| {
                    let first =
                        cx.new(|cx| EditorView::new_with_watch(first_pane, false, window, cx));
                    let second = second_pane.map(|pane| {
                        cx.new(|cx| EditorView::new_with_watch(pane, false, window, cx))
                    });
                    let terminal = cx.new(|cx| {
                        super::super::terminal::TerminalView::new(
                            terminal_pane,
                            Err::<Arc<ShellEnvironment>, _>("test terminal".into()),
                            cx,
                        )
                    });
                    cx.new(|_| Harness {
                        first,
                        second,
                        terminal,
                    })
                })
                .unwrap();
            let harness = window.root(cx).unwrap();
            let first = harness.read(cx).first.clone();
            let second = harness.read(cx).second.clone();
            (window, first, second)
        })
    }

    #[gpui_kit::test]
    fn windows_ctrl_s_saves_the_focused_real_editor(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_owned();
        std::fs::write(root.join("first.txt"), "initial").unwrap();
        let (window, editor, _) = open(&root, None, cx);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let focus = editor.read_with(&cx, |view, cx| view.input.read(cx).focus_handle(cx));
        cx.update(|window, cx| focus.focus(window, cx));
        cx.simulate_input(" changed");
        let expected = editor.read_with(&cx, |view, cx| view.input.read(cx).value().to_string());
        assert!(editor.read_with(&cx, |view, _| view.dirty));

        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();

        assert_eq!(
            std::fs::read_to_string(root.join("first.txt")).unwrap(),
            expected
        );
        assert!(!editor.read_with(&cx, |view, _| view.dirty));
    }

    #[gpui_kit::test]
    fn ctrl_s_keeps_a_conflicted_buffer_and_surfaces_the_error(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_owned();
        let path = root.join("first.txt");
        std::fs::write(&path, "initial").unwrap();
        let (window, editor, _) = open(&root, None, cx);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let focus = editor.read_with(&cx, |view, cx| view.input.read(cx).focus_handle(cx));
        cx.update(|window, cx| focus.focus(window, cx));
        cx.simulate_input(" local");
        std::fs::write(&path, "external").unwrap();
        cx.update(|window, cx| editor.update(cx, |view, cx| view.reload(window, cx)));
        cx.run_until_parked();
        assert!(editor.read_with(&cx, |view, _| view.conflict));

        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
        editor.read_with(&cx, |view, _| {
            assert!(view.dirty);
            assert!(
                view.error
                    .as_deref()
                    .is_some_and(|error| error.contains("Resolve"))
            );
        });
    }

    #[gpui_kit::test]
    fn ctrl_s_does_not_save_an_unfocused_editor_or_escape_terminal_focus(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_owned();
        std::fs::write(root.join("first.txt"), "first").unwrap();
        std::fs::write(root.join("second.txt"), "second").unwrap();
        let (window, first, second) = open(&root, Some("second.txt"), cx);
        let second = second.unwrap();
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let first_focus = first.read_with(&cx, |view, cx| view.input.read(cx).focus_handle(cx));
        let second_focus = second.read_with(&cx, |view, cx| view.input.read(cx).focus_handle(cx));
        cx.update(|window, cx| first_focus.focus(window, cx));
        cx.simulate_input(" edit");
        let first_expected =
            first.read_with(&cx, |view, cx| view.input.read(cx).value().to_string());
        cx.update(|window, cx| second_focus.focus(window, cx));
        cx.simulate_input(" edit");
        let second_expected =
            second.read_with(&cx, |view, cx| view.input.read(cx).value().to_string());
        cx.update(|window, cx| first_focus.focus(window, cx));
        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();
        assert_eq!(
            std::fs::read_to_string(root.join("first.txt")).unwrap(),
            first_expected
        );
        assert_eq!(
            std::fs::read_to_string(root.join("second.txt")).unwrap(),
            "second"
        );
        assert!(second.read_with(&cx, |view, _| view.dirty));

        cx.update(|window, cx| first_focus.focus(window, cx));
        cx.simulate_input(" unsaved");
        cx.update(|window, cx| {
            let terminal_focus = window
                .root::<Harness>()
                .unwrap()
                .unwrap()
                .read(cx)
                .terminal
                .read(cx)
                .focus_handle();
            terminal_focus.focus(window, cx);
        });
        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();
        assert_eq!(
            std::fs::read_to_string(root.join("first.txt")).unwrap(),
            first_expected
        );
        assert!(first.read_with(&cx, |view, _| view.dirty));
        assert_ne!(second_expected, "second");
    }
}
