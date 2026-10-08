//! An in-memory file tree: stable IDs, prepared visible rows, and virtualized rendering.
//! Loading files belongs to the caller; this component never touches the filesystem.
use super::super::theme as t;
use super::files::{TREE_INDENT, TREE_INSET, tree_indent_guides};
use super::{column, icon, icon_button, row};
use gpui_kit::*;
use gpui_kit::{base::Disableable, component::IconName};
use std::collections::HashSet;
use std::time::Instant;
mod motion;
use motion::TreeMotion;

#[derive(Clone, Debug)]
pub struct FileNode {
    pub id: SharedString,
    pub label: SharedString,
    /// None is a file; Some(empty) is an empty folder.
    pub children: Option<Vec<FileNode>>,
    pub git_status: Option<char>,
    pub ignored: bool,
    pub loading: bool,
    pub error: Option<String>,
}
impl FileNode {
    pub fn file(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: None,
            git_status: None,
            ignored: false,
            loading: false,
            error: None,
        }
    }
    pub fn folder(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        children: Vec<Self>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Some(children),
            git_status: None,
            ignored: false,
            loading: false,
            error: None,
        }
    }
}
#[derive(Clone, Debug)]
struct VisibleRow {
    expanded: bool,
    id: SharedString,
    label: SharedString,
    depth: usize,
    folder: bool,
    git_status: Option<char>,
    ignored: bool,
    loading: bool,
    error: Option<String>,
}

pub struct FileTreeModel {
    nodes: Vec<FileNode>,
    expanded: HashSet<SharedString>,
    selected: Option<SharedString>,
    visible: Vec<VisibleRow>,
}
impl FileTreeModel {
    pub fn new(
        nodes: Vec<FileNode>,
        expanded: impl IntoIterator<Item = SharedString>,
    ) -> Result<Self, String> {
        fn validate(nodes: &[FileNode], ids: &mut HashSet<SharedString>) -> Result<(), String> {
            for node in nodes {
                if !ids.insert(node.id.clone()) {
                    return Err(format!("Duplicate tree ID: {}", node.id));
                }
                if let Some(children) = &node.children {
                    validate(children, ids)?;
                }
            }
            Ok(())
        }
        validate(&nodes, &mut HashSet::new())?;
        let mut model = Self {
            nodes,
            expanded: expanded.into_iter().collect(),
            selected: None,
            visible: Vec::new(),
        };
        model.rebuild();
        Ok(model)
    }
    fn rebuild(&mut self) {
        fn visit(
            nodes: &[FileNode],
            depth: usize,
            expanded: &HashSet<SharedString>,
            out: &mut Vec<VisibleRow>,
        ) {
            for node in nodes {
                out.push(VisibleRow {
                    expanded: node.children.is_some() && expanded.contains(&node.id),
                    id: node.id.clone(),
                    label: node.label.clone(),
                    depth,
                    folder: node.children.is_some(),
                    git_status: node.git_status,
                    ignored: node.ignored,
                    loading: node.loading,
                    error: node.error.clone(),
                });
                if expanded.contains(&node.id)
                    && let Some(children) = &node.children
                {
                    visit(children, depth + 1, expanded, out);
                }
            }
        }
        self.visible.clear();
        visit(&self.nodes, 0, &self.expanded, &mut self.visible);
    }
    fn toggle(&mut self, id: &SharedString) -> bool {
        if !self.visible.iter().any(|row| &row.id == id && row.folder) {
            return false;
        }
        if !self.expanded.remove(id) {
            self.expanded.insert(id.clone());
        }
        self.rebuild();
        // Collapsing a selected descendant moves selection to its visible folder.
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| !self.visible.iter().any(|r| &r.id == selected))
        {
            self.selected = Some(id.clone());
        }
        true
    }
}

#[derive(Clone, Debug)]
pub enum FileTreeRequest {
    Expanded(Vec<SharedString>),
    Refresh,
}

#[derive(Clone, Debug)]
pub struct FileTreeEvent {
    pub id: SharedString,
}
actions!(file_tree, [Up, Down, Left, Right, Open]);

pub struct FileTree {
    model: FileTreeModel,
    focus: FocusHandle,
    scroll: ScrollHandle,
    content_top: f32,
    viewport_top: f32,
    viewport_height: f32,
    layout: TreeMotion,
}
impl EventEmitter<FileTreeEvent> for FileTree {}
impl EventEmitter<FileTreeRequest> for FileTree {}
impl FileTree {
    pub fn new(model: FileTreeModel, cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("up", Up, Some("FileTree")),
            KeyBinding::new("down", Down, Some("FileTree")),
            KeyBinding::new("left", Left, Some("FileTree")),
            KeyBinding::new("right", Right, Some("FileTree")),
            KeyBinding::new("enter", Open, Some("FileTree")),
        ]);
        Self {
            layout: TreeMotion::new(&model.visible, Instant::now()),
            model,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            content_top: 0.,
            viewport_top: 0.,
            viewport_height: 0.,
        }
    }
    pub fn replace(&mut self, nodes: Vec<FileNode>, reset: bool, cx: &mut Context<Self>) {
        let expanded = if reset {
            vec![]
        } else {
            self.model.expanded.iter().cloned().collect()
        };
        if let Ok(mut model) = FileTreeModel::new(nodes, expanded) {
            if !reset {
                model.selected = self
                    .model
                    .selected
                    .clone()
                    .filter(|id| model.visible.iter().any(|r| &r.id == id));
            }
            let same = model.visible.iter().map(|r| (&r.id, r.expanded)).eq(self
                .model
                .visible
                .iter()
                .map(|r| (&r.id, r.expanded)));
            if reset {
                self.layout = TreeMotion::new(&model.visible, Instant::now());
            } else if same {
                self.layout.update_entries(&model.visible);
            } else {
                self.layout.retarget(
                    &model.visible,
                    model.visible.len() >= self.model.visible.len(),
                    Instant::now(),
                    canopy_desktop::motion::policy(cx),
                );
            }
            self.model = model;
            self.request_directories(cx);
            cx.notify();
        }
    }
    fn request_directories(&self, cx: &mut Context<Self>) {
        cx.emit(FileTreeRequest::Expanded(
            self.model
                .visible
                .iter()
                .filter(|r| r.folder && r.expanded)
                .map(|r| r.id.clone())
                .collect(),
        ));
    }
    pub fn use_sidebar_scroll(&mut self, scroll: ScrollHandle) {
        self.scroll = scroll;
    }
    pub fn animated_height(&self, now: Instant) -> f32 {
        self.layout.height(now).max(t::ROW)
    }
    pub fn is_animating(&self, now: Instant) -> bool {
        self.layout.active(now)
    }
    fn toggle(&mut self, id: &SharedString, cx: &mut Context<Self>) {
        if !self.model.toggle(id) {
            return;
        }
        self.request_directories(cx);
        self.layout.retarget(
            &self.model.visible,
            self.model.expanded.contains(id),
            Instant::now(),
            canopy_desktop::motion::policy(cx),
        );
        cx.notify();
    }
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.model.visible.is_empty() {
            return;
        }
        let current = self
            .model
            .visible
            .iter()
            .position(|r| Some(&r.id) == self.model.selected.as_ref());
        let index = current
            .map(|i| {
                i.saturating_add_signed(delta)
                    .min(self.model.visible.len() - 1)
            })
            .unwrap_or(0);
        self.model.selected = Some(self.model.visible[index].id.clone());
        let top = self.content_top + index as f32 * t::ROW;
        let offset = -f32::from(self.scroll.offset().y);
        let height = f32::from(self.scroll.bounds().size.height);
        if top < offset {
            self.scroll.set_offset(point(px(0.), px(-top)));
        } else if top + t::ROW > offset + height {
            self.scroll
                .set_offset(point(px(0.), px(-(top + t::ROW - height))));
        }
        cx.notify();
    }
    fn open(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.model.selected.clone() {
            if self.model.visible.iter().any(|r| r.id == id && r.folder) {
                self.toggle(&id, cx);
            } else {
                cx.emit(FileTreeEvent { id });
            }
            cx.notify();
        }
    }
}
impl Render for FileTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.layout.retire_exiting(now);
        canopy_desktop::motion::request_frame(window, self.layout.active(now));
        let top = self.viewport_top;
        let viewport = self.viewport_height;
        let previous_content_top = self.content_top;
        let expected_height = viewport;
        let owner = cx.entity().downgrade();
        let scroll = self.scroll.clone();
        column()
            .id("file-tree")
            .relative()
            .w_full()
            .h(px(self.animated_height(now)))
            .flex_shrink_0()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let viewport = scroll.bounds();
                        let visible_top = f32::from(viewport.top() - bounds.top()).max(0.);
                        let height = f32::from(viewport.size.height);
                        let content_top =
                            f32::from(bounds.top() - viewport.top() - scroll.offset().y);
                        if (top - visible_top).abs() <= 0.5
                            && (expected_height - height).abs() <= 0.5
                            && (previous_content_top - content_top).abs() <= 0.5
                        {
                            return;
                        }
                        let owner = owner.clone();
                        window.on_next_frame(move |_, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                if (this.content_top - content_top).abs() > 0.5
                                    || (this.viewport_top - visible_top).abs() > 0.5
                                    || (this.viewport_height - height).abs() > 0.5
                                {
                                    this.content_top = content_top;
                                    this.viewport_top = visible_top;
                                    this.viewport_height = height;
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .track_focus(&self.focus)
            .key_context("FileTree")
            .on_action(cx.listener(|this, _: &Up, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &Open, _, cx| this.open(cx)))
            .on_action(cx.listener(|this, _: &Left, _, cx| {
                if let Some(id) = this.model.selected.clone() {
                    if this.model.expanded.contains(&id) {
                        this.toggle(&id, cx);
                    } else if let Some(index) = this.model.visible.iter().position(|r| r.id == id) {
                        let depth = this.model.visible[index].depth;
                        if let Some(parent) = this.model.visible[..index]
                            .iter()
                            .rfind(|r| r.depth < depth)
                        {
                            this.model.selected = Some(parent.id.clone());
                        }
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &Right, _, cx| {
                if let Some(id) = this.model.selected.clone() {
                    if !this.model.expanded.contains(&id) {
                        this.toggle(&id, cx);
                    } else if let Some(index) = this.model.visible.iter().position(|r| r.id == id)
                        && this
                            .model
                            .visible
                            .get(index + 1)
                            .is_some_and(|next| next.depth > this.model.visible[index].depth)
                    {
                        this.move_selection(1, cx);
                    }
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("file-tree-rows")
                    .size_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(self.layout.height(now)))
                            .children(
                                self.layout
                                    .range((top - 2. * t::ROW).max(0.), viewport + 4. * t::ROW, now)
                                    .filter_map(|index| {
                                        let alpha = self.layout.alpha(index, now);
                                        if alpha <= 0.001 {
                                            return None;
                                        }
                                        let item = &self.layout.rows[index];
                                        let entry = item.entry.clone();
                                        let id = entry.id.clone();
                                        let interactive = item.interactive;
                                        Some(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .top(px(self.layout.y(index, now)))
                                                .w_full()
                                                .h(px(t::ROW))
                                                .opacity(alpha)
                                                .child(
                                                    row()
                                                        .id(entry.id.clone())
                                                        .relative()
                                                        .w_full()
                                                        .h(px(t::ROW))
                                                        .gap(px(8.))
                                                        .pl(px(TREE_INSET
                                                            + entry.depth as f32 * TREE_INDENT))
                                                        .pr(px(8.))
                                                        .cursor_pointer()
                                                        .rounded(px(3.))
                                                        .bg(
                                                            if self.model.selected.as_ref()
                                                                == Some(&entry.id)
                                                            {
                                                                t::selected()
                                                            } else {
                                                                rgba(0).into()
                                                            },
                                                        )
                                                        .hover(|s| s.bg(t::hover()))
                                                        .text_color(
                                                            entry
                                                                .git_status
                                                                .map(git_color)
                                                                .unwrap_or_else(|| {
                                                                    if entry.ignored {
                                                                        t::muted()
                                                                    } else {
                                                                        t::text()
                                                                    }
                                                                }),
                                                        )
                                                        .child(
                                                            icon(if entry.folder {
                                                                IconName::ChevronRight
                                                            } else {
                                                                IconName::File
                                                            })
                                                            .size(px(12.))
                                                            .text_color(if entry.ignored {
                                                                t::faint()
                                                            } else {
                                                                t::muted()
                                                            })
                                                            .rotate(radians(
                                                                self.layout.rotation(index, now)
                                                                    * std::f32::consts::FRAC_PI_2,
                                                            )),
                                                        )
                                                        .children(entry.folder.then(|| {
                                                            icon(IconName::Folder).text_color(
                                                                entry
                                                                    .git_status
                                                                    .map(git_color)
                                                                    .unwrap_or_else(|| {
                                                                        if entry.ignored {
                                                                            t::faint()
                                                                        } else {
                                                                            t::muted()
                                                                        }
                                                                    }),
                                                            )
                                                        }))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .truncate()
                                                                .child(entry.label.clone()),
                                                        )
                                                        .children(
                                                            entry
                                                                .git_status
                                                                .filter(|_| !entry.folder)
                                                                .map(|status| {
                                                                    div()
                                                                        .text_color(git_color(
                                                                            status,
                                                                        ))
                                                                        .text_size(px(10.))
                                                                        .child(status.to_string())
                                                                }),
                                                        )
                                                        .children(entry.loading.then(|| {
                                                            div()
                                                                .text_size(px(10.))
                                                                .text_color(t::muted())
                                                                .child("Loading…")
                                                        }))
                                                        .children(entry.error.clone().map(
                                                            |error| {
                                                                icon_button(
                                                            SharedString::from(format!(
                                                                "retry-{}",
                                                                entry.id
                                                            )),
                                                            IconName::RotateCw,
                                                            error,
                                                        )
                                                        .disabled(!interactive)
                                                        .on_click(cx.listener(|_, _, _, cx| {
                                                            cx.stop_propagation();
                                                            cx.emit(FileTreeRequest::Refresh);
                                                        }))
                                                            },
                                                        ))
                                                        .child(tree_indent_guides(entry.depth))
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(|this, _, window, cx| {
                                                                this.focus.focus(window, cx)
                                                            }),
                                                        )
                                                        .on_click(cx.listener(
                                                            move |this, _: &ClickEvent, _, cx| {
                                                                if !interactive {
                                                                    return;
                                                                }
                                                                this.model.selected =
                                                                    Some(id.clone());
                                                                if entry.folder {
                                                                    this.toggle(&id, cx);
                                                                } else {
                                                                    cx.emit(FileTreeEvent {
                                                                        id: id.clone(),
                                                                    });
                                                                }
                                                                cx.notify();
                                                            },
                                                        )),
                                                ),
                                        )
                                    }),
                            ),
                    ),
            )
    }
}

fn git_color(status: char) -> Hsla {
    match status {
        'A' => t::green(),
        'D' | 'U' => t::red(),
        'R' => t::accent(),
        _ => t::yellow(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FileNode, FileTreeModel};
    fn fixture() -> Vec<FileNode> {
        vec![
            FileNode::folder("src", "src", vec![FileNode::file("src/a.rs", "a.rs")]),
            FileNode::file("a.rs", "a.rs"),
        ]
    }
    #[test]
    fn collapse_preserves_identity_and_moves_hidden_selection() {
        let mut model = FileTreeModel::new(fixture(), ["src".into()]).unwrap();
        assert_eq!(model.visible.len(), 3);
        model.selected = Some("src/a.rs".into());
        model.toggle(&"src".into());
        assert_eq!(model.selected.as_deref(), Some("src"));
        assert_eq!(model.visible.len(), 2);
        model.toggle(&"src".into());
        assert_eq!(model.visible[1].id.as_ref(), "src/a.rs");
        assert_eq!(model.visible[2].id.as_ref(), "a.rs");
    }
    #[test]
    fn rejects_duplicate_ids_not_duplicate_labels() {
        assert!(FileTreeModel::new(fixture(), []).is_ok());
        assert!(
            FileTreeModel::new(
                vec![FileNode::file("same", "one"), FileNode::file("same", "two")],
                []
            )
            .is_err()
        );
    }
    #[test]
    fn empty_folder_is_not_a_file_and_file_cannot_expand() {
        let mut model = FileTreeModel::new(
            vec![
                FileNode::folder("empty", "empty", vec![]),
                FileNode::file("file", "file"),
            ],
            [],
        )
        .unwrap();
        model.toggle(&"file".into());
        assert!(!model.expanded.contains("file"));
        model.toggle(&"empty".into());
        assert!(model.expanded.contains("empty"));
        assert_eq!(model.visible.len(), 2);
    }
}
