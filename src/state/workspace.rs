use serde::{Deserialize, Serialize};
use std::path::PathBuf;
// Each tab owns a split tree; leaves own session metadata.
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
        pub struct $name(u64);
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            fn counter() -> &'static AtomicU64 {
                static NEXT: AtomicU64 = AtomicU64::new(1);
                &NEXT
            }
            pub fn new() -> Self {
                Self(Self::counter().fetch_add(1, Ordering::Relaxed))
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let value = u64::deserialize(d)?;
                if value == 0 || value >= u64::MAX / 2 {
                    return Err(serde::de::Error::custom("invalid identifier"));
                }
                Self::counter().fetch_max(value + 1, Ordering::Relaxed);
                Ok(Self(value))
            }
        }
    };
}
id!(ProjectId);
id!(WorkspaceId);
id!(TabId);
id!(PaneId);
id!(SplitId);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
    pub id: PaneId,
    pub tool: String,
    #[serde(default)]
    pub metadata: PaneMetadata,
}
/// Durable launch/view metadata; never contains a PID, PTY handle or running flag.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaneMetadata {
    pub cwd: Option<PathBuf>,
    pub profile_id: Option<String>,
    pub title: Option<String>,
    pub resource: Option<String>,
    pub kind: PaneKind,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub resume_id: Option<String>,
    /// Unsent task draft. Cleared after a successful bracketed paste; never a CLI argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_prompt: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PaneKind {
    #[default]
    Terminal,
    Browser,
    Editor,
    Image,
    Font,
    Video,
    Diff,
    Notes,
}
impl PaneMetadata {
    fn valid(&self) -> bool {
        self.cwd.as_ref().is_none_or(|p| p.is_absolute())
            && self
                .task_prompt
                .as_ref()
                .is_none_or(|s| s.len() <= 512 * 1024 && !s.contains('\0'))
            && self.arguments.len() <= 128
            && self.arguments.iter().all(|a| a.len() <= 16384)
            && [
                &self.title,
                &self.resource,
                &self.profile_id,
                &self.resume_id,
            ]
            .iter()
            .all(|v| v.as_ref().is_none_or(|s| s.len() <= 16384))
    }
}
impl Pane {
    fn new(tool: impl Into<String>) -> Self {
        Self {
            id: PaneId::new(),
            tool: tool.into(),
            metadata: PaneMetadata::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SplitTree {
    Leaf(Pane),
    Split {
        id: SplitId,
        axis: Axis,
        ratio: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}
impl SplitTree {
    fn collect(&self, out: &mut Vec<Pane>) {
        match self {
            Self::Leaf(p) => out.push(p.clone()),
            Self::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }
    fn fill_cwd(&mut self, path: &PathBuf) {
        match self {
            Self::Leaf(p) => {
                if p.metadata.cwd.is_none() {
                    p.metadata.cwd = Some(path.clone());
                }
            }
            Self::Split { first, second, .. } => {
                first.fill_cwd(path);
                second.fill_cwd(path);
            }
        }
    }
    fn valid(
        &self,
        depth: usize,
        panes: &mut std::collections::HashSet<PaneId>,
        splits: &mut std::collections::HashSet<SplitId>,
    ) -> bool {
        if depth > 4 {
            return false;
        }
        match self {
            Self::Leaf(p) => {
                panes.insert(p.id)
                    && !p.tool.trim().is_empty()
                    && p.tool.len() <= 1024
                    && p.metadata.valid()
            }
            Self::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                splits.insert(*id)
                    && ratio.is_finite()
                    && (0.1..=0.9).contains(ratio)
                    && first.valid(depth + 1, panes, splits)
                    && second.valid(depth + 1, panes, splits)
            }
        }
    }
    pub fn panes(&self) -> Vec<Pane> {
        let mut panes = vec![];
        self.collect(&mut panes);
        panes
    }
    pub fn first(&self) -> &Pane {
        match self {
            Self::Leaf(p) => p,
            Self::Split { first, .. } => first.first(),
        }
    }
    pub fn find(&self, id: PaneId) -> Option<&Pane> {
        match self {
            Self::Leaf(p) => (p.id == id).then_some(p),
            Self::Split { first, second, .. } => first.find(id).or_else(|| second.find(id)),
        }
    }
    fn depth(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Split { first, second, .. } => 1 + first.depth().max(second.depth()),
        }
    }
    fn split(&mut self, target: PaneId, axis: Axis, new: Pane) -> bool {
        match self {
            Self::Leaf(p) if p.id == target => {
                *self = Self::Split {
                    id: SplitId::new(),
                    axis,
                    ratio: 0.5,
                    first: Box::new(self.clone()),
                    second: Box::new(Self::Leaf(new)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                if first.find(target).is_some() {
                    first.split(target, axis, new)
                } else {
                    second.split(target, axis, new)
                }
            }
        }
    }
    fn replace(&mut self, target: PaneId, replacement: Self) -> bool {
        match self {
            Self::Leaf(p) if p.id == target => {
                *self = replacement;
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                if first.find(target).is_some() {
                    first.replace(target, replacement)
                } else {
                    second.replace(target, replacement)
                }
            }
        }
    }
    fn graft(&mut self, target: PaneId, axis: Axis, new: Self, before: bool) -> bool {
        let Some(original) = self.find(target).cloned() else {
            return false;
        };
        let old = Box::new(Self::Leaf(original));
        let new = Box::new(new);
        let (first, second) = if before { (new, old) } else { (old, new) };
        self.replace(
            target,
            Self::Split {
                id: SplitId::new(),
                axis,
                ratio: 0.5,
                first,
                second,
            },
        )
    }
    fn remove(self, target: PaneId) -> Option<Self> {
        match self {
            Self::Leaf(p) => (p.id != target).then_some(Self::Leaf(p)),
            Self::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(target), second.remove(target)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    id,
                    axis,
                    ratio,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (a, b) => a.or(b),
            },
        }
    }
    fn ratio(&mut self, target: SplitId, value: f32) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                if *id == target {
                    *ratio = value.clamp(0.1, 0.9);
                    true
                } else {
                    first.ratio(target, value) || second.ratio(target, value)
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub root: SplitTree,
    pub focused: PaneId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: WorkspaceId,
    tabs: Vec<Tab>,
    active: Option<TabId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    MissingTab,
    MissingPane,
    MissingSplit,
    DepthLimit,
    InvalidRatio,
    SameTab,
    InvalidMetadata,
    InvalidTitle,
}
impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}
impl Workspace {
    pub fn new() -> Self {
        Self {
            id: WorkspaceId::new(),
            tabs: vec![],
            active: None,
        }
    }
    /// New project/worktree contexts are empty until the user explicitly opens a tool.
    pub fn empty(id: WorkspaceId) -> Self {
        Self {
            id,
            tabs: Vec::new(),
            active: None,
        }
    }
    /// Bind a provider session only while pane identity still matches the launch.
    pub fn bind_agent_session(&mut self, source: &Pane, session: &str) -> Result<bool, StateError> {
        let (tab, pane) = self
            .tabs
            .iter()
            .find_map(|tab| tab.root.find(source.id).map(|pane| (tab.id, pane.clone())))
            .ok_or(StateError::MissingPane)?;
        if pane.tool != source.tool
            || pane.metadata.cwd != source.metadata.cwd
            || pane.metadata.profile_id != source.metadata.profile_id
        {
            return Err(StateError::InvalidMetadata);
        }
        if pane.metadata.resume_id.as_deref() == Some(session) {
            return Ok(false);
        }
        if pane.metadata.resume_id != source.metadata.resume_id {
            return Err(StateError::InvalidMetadata);
        }
        let mut metadata = pane.metadata;
        metadata.resume_id = Some(session.to_owned());
        self.set_pane_metadata(tab, pane.id, metadata)?;
        Ok(true)
    }
    pub fn set_pane_metadata(
        &mut self,
        tab: TabId,
        pane: PaneId,
        metadata: PaneMetadata,
    ) -> Result<(), StateError> {
        if !metadata.valid() {
            return Err(StateError::InvalidMetadata);
        }
        let tab = self.tab_mut(tab)?;
        let mut value = tab
            .root
            .find(pane)
            .cloned()
            .ok_or(StateError::MissingPane)?;
        value.metadata = metadata;
        tab.root.replace(pane, SplitTree::Leaf(value));
        Ok(())
    }
    pub fn set_default_cwd(&mut self, path: PathBuf) {
        for tab in &mut self.tabs {
            tab.root.fill_cwd(&path);
        }
    }
    pub fn all_panes(&self) -> Vec<Pane> {
        let mut panes = vec![];
        for tab in &self.tabs {
            tab.root.collect(&mut panes);
        }
        panes
    }
    pub fn activation_plan(&self) -> Vec<Pane> {
        let mut panes = vec![];
        if let Some(tab) = self.active() {
            tab.root.collect(&mut panes);
        }
        panes
    }
    pub fn valid(&self) -> bool {
        if self.tabs.len() > 256 || self.active.is_some() == self.tabs.is_empty() {
            return false;
        }
        if self.active.is_some() && self.active().is_none() {
            return false;
        }
        let mut tabs = std::collections::HashSet::new();
        let mut panes = std::collections::HashSet::new();
        let mut splits = std::collections::HashSet::new();
        self.tabs.iter().all(|tab| {
            tabs.insert(tab.id)
                && tab.title.len() <= 16384
                && tab.root.find(tab.focused).is_some()
                && tab.root.valid(1, &mut panes, &mut splits)
        })
    }
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }
    pub fn active(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| Some(t.id) == self.active)
    }
    /// Navigate to a pane using its current tab, including after drag/drop.
    pub fn activate_pane(&mut self, pane: PaneId) -> Result<(), StateError> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.root.find(pane).is_some())
            .map(|tab| tab.id)
            .ok_or(StateError::MissingPane)?;
        self.activate(tab)?;
        self.focus(tab, pane)
    }
    pub fn activate(&mut self, id: TabId) -> Result<(), StateError> {
        if !self.tabs.iter().any(|t| t.id == id) {
            return Err(StateError::MissingTab);
        }
        self.active = Some(id);
        Ok(())
    }
    pub fn open(&mut self, title: impl Into<String>, tool: impl Into<String>) -> TabId {
        let pane = Pane::new(tool);
        let tab = Tab {
            id: TabId::new(),
            title: title.into(),
            focused: pane.id,
            root: SplitTree::Leaf(pane),
        };
        let id = tab.id;
        self.tabs.push(tab);
        self.active = Some(id);
        id
    }
    pub fn rename(&mut self, id: TabId, title: impl Into<String>) -> Result<(), StateError> {
        let title = title.into();
        let title = title.trim();
        if title.is_empty() || title.len() > 16384 || title.chars().any(char::is_control) {
            return Err(StateError::InvalidTitle);
        }
        self.tab_mut(id)?.title = title.to_owned();
        Ok(())
    }
    pub fn close(&mut self, id: TabId) -> Result<(), StateError> {
        let index = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or(StateError::MissingTab)?;
        self.tabs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|t| t.id);
        }
        Ok(())
    }
    fn tab_mut(&mut self, id: TabId) -> Result<&mut Tab, StateError> {
        self.tabs
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(StateError::MissingTab)
    }
    pub fn focus(&mut self, tab: TabId, pane: PaneId) -> Result<(), StateError> {
        let t = self.tab_mut(tab)?;
        if t.root.find(pane).is_none() {
            return Err(StateError::MissingPane);
        }
        t.focused = pane;
        self.active = Some(tab);
        Ok(())
    }
    pub fn split(
        &mut self,
        tab: TabId,
        pane: PaneId,
        axis: Axis,
        tool: impl Into<String>,
    ) -> Result<PaneId, StateError> {
        let t = self.tab_mut(tab)?;
        if t.root.find(pane).is_none() {
            return Err(StateError::MissingPane);
        }
        if t.root.depth() >= 4 {
            return Err(StateError::DepthLimit);
        }
        let mut new = Pane::new(tool);
        new.metadata.cwd = t.root.find(pane).and_then(|p| p.metadata.cwd.clone());
        let id = new.id;
        t.root.split(pane, axis, new);
        t.focused = id;
        Ok(id)
    }
    pub fn close_pane(&mut self, tab: TabId, pane: PaneId) -> Result<(), StateError> {
        let t = self.tab_mut(tab)?;
        if t.root.find(pane).is_none() {
            return Err(StateError::MissingPane);
        }
        if let Some(root) = t.root.clone().remove(pane) {
            if t.focused == pane {
                t.focused = root.first().id;
            }
            t.root = root;
            Ok(())
        } else {
            self.close(tab)
        }
    }
    pub fn set_ratio(&mut self, tab: TabId, split: SplitId, ratio: f32) -> Result<(), StateError> {
        if !ratio.is_finite() {
            return Err(StateError::InvalidRatio);
        }
        if self.tab_mut(tab)?.root.ratio(split, ratio) {
            Ok(())
        } else {
            Err(StateError::MissingSplit)
        }
    }
    pub fn move_pane(
        &mut self,
        source: TabId,
        pane: PaneId,
        destination: TabId,
        target: PaneId,
        axis: Axis,
    ) -> Result<(), StateError> {
        self.dock_pane(source, pane, destination, target, axis, false)
    }
    /// Edit a candidate first: invalid destinations or depth limits never destroy a session.
    pub fn dock_pane(
        &mut self,
        source: TabId,
        pane: PaneId,
        destination: TabId,
        target: PaneId,
        axis: Axis,
        before: bool,
    ) -> Result<(), StateError> {
        if pane == target {
            return Err(StateError::MissingPane);
        }
        let mut next = self.clone();
        let value = next
            .tab_mut(source)?
            .root
            .find(pane)
            .cloned()
            .ok_or(StateError::MissingPane)?;
        next.close_pane(source, pane)?;
        let dest = next.tab_mut(destination)?;
        if !dest
            .root
            .graft(target, axis, SplitTree::Leaf(value), before)
        {
            return Err(StateError::MissingPane);
        }
        if dest.root.depth() > 4 {
            return Err(StateError::DepthLimit);
        }
        dest.focused = pane;
        next.active = Some(destination);
        *self = next;
        Ok(())
    }
    pub fn detach_pane(&mut self, source: TabId, pane: PaneId) -> Result<TabId, StateError> {
        let value = self
            .tab_mut(source)?
            .root
            .find(pane)
            .cloned()
            .ok_or(StateError::MissingPane)?;
        if matches!(self.tab_mut(source)?.root, SplitTree::Leaf(_)) {
            self.active = Some(source);
            return Ok(source);
        }
        self.close_pane(source, pane)?;
        let id = TabId::new();
        self.tabs.push(Tab {
            id,
            title: value.tool.clone(),
            root: SplitTree::Leaf(value),
            focused: pane,
        });
        self.active = Some(id);
        Ok(id)
    }
    pub fn dock_tab(
        &mut self,
        source: TabId,
        destination: TabId,
        target: PaneId,
        axis: Axis,
        before: bool,
    ) -> Result<(), StateError> {
        if source == destination {
            return Err(StateError::SameTab);
        }
        let mut next = self.clone();
        let source_tab = next.tab_mut(source)?.clone();
        let dest = next.tab_mut(destination)?;
        if !dest.root.graft(target, axis, source_tab.root, before) {
            return Err(StateError::MissingPane);
        }
        if dest.root.depth() > 4 {
            return Err(StateError::DepthLimit);
        }
        dest.focused = source_tab.focused;
        next.close(source)?;
        next.active = Some(destination);
        *self = next;
        Ok(())
    }
    pub fn swap_panes(
        &mut self,
        source: TabId,
        pane: PaneId,
        destination: TabId,
        target: PaneId,
    ) -> Result<(), StateError> {
        let a = self
            .tab_mut(source)?
            .root
            .find(pane)
            .cloned()
            .ok_or(StateError::MissingPane)?;
        let b = self
            .tab_mut(destination)?
            .root
            .find(target)
            .cloned()
            .ok_or(StateError::MissingPane)?;
        if pane == target {
            return Ok(());
        }
        if source == destination {
            // Replace with a temporary unique identity to avoid matching the moved leaf twice.
            let temporary = Pane::new("");
            let id = temporary.id;
            let tab = self.tab_mut(source)?;
            tab.root.replace(pane, SplitTree::Leaf(temporary));
            tab.root.replace(target, SplitTree::Leaf(a));
            tab.root.replace(id, SplitTree::Leaf(b));
        } else {
            let first = self.tab_mut(source)?;
            first.root.replace(pane, SplitTree::Leaf(b));
            if first.focused == pane {
                first.focused = target;
            }
            let second = self.tab_mut(destination)?;
            second.root.replace(target, SplitTree::Leaf(a));
        }
        self.tab_mut(destination)?.focused = pane;
        self.active = Some(destination);
        Ok(())
    }
    pub fn reorder_tab(&mut self, id: TabId, before: Option<TabId>) -> Result<(), StateError> {
        if before == Some(id) {
            return Ok(());
        }
        if before.is_some_and(|target| !self.tabs.iter().any(|t| t.id == target)) {
            return Err(StateError::MissingTab);
        }
        let index = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or(StateError::MissingTab)?;
        let tab = self.tabs.remove(index);
        let dest = before
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(dest, tab);
        Ok(())
    }
    /// Move a complete tab, including its split tree and identities, between workspaces.
    pub fn move_tab(&mut self, tab: TabId, destination: &mut Self) -> Result<(), StateError> {
        let value = self
            .tabs
            .iter()
            .find(|t| t.id == tab)
            .cloned()
            .ok_or(StateError::MissingTab)?;
        self.close(tab)?;
        destination.tabs.push(value);
        destination.active = Some(tab);
        Ok(())
    }
}
