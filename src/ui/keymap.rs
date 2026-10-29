//! User-facing keyboard commands: one table with their names, scopes and default
//! shortcuts per platform, plus the user's overrides (`keyboard.shortcuts`).
//!
//! Every command below is bound from this table; its shortcut can be changed in
//! Settings → Shortcuts. Keys that belong to a component's own interaction (list
//! arrows, Escape dismissals, keys forwarded to the terminal, text editing) are
//! fixed: see `fixed_bindings` and the components' own `bind_keys`.
//!
//! Rebinding never clears bindings registered by others (GPUI Component inputs,
//! menus, lists, …): ours carry the `OURS` meta index, and a rebind keeps every
//! other binding in its original order and appends the new set of ours.
use crate::app_state::AppState;
use canopy_desktop::settings::{Change, ShortcutOverrides};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// Marks the bindings built from `COMMANDS`, so a rebind can replace only them.
/// Meta only matters to `NoAction` precedence, where every binding without a
/// meta counts as 0; any `NoAction` registered today still hides ours.
const OURS: KeyBindingMetaIndex = KeyBindingMetaIndex(0x4b4d);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}
impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    App,
    Chat,
    TabsAndPanes,
    View,
    Files,
    Terminal,
}
impl Group {
    pub const ALL: [Self; 6] = [
        Self::App,
        Self::Chat,
        Self::TabsAndPanes,
        Self::View,
        Self::Files,
        Self::Terminal,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::App => "APP",
            Self::Chat => "CHAT",
            Self::TabsAndPanes => "TABS & PANES",
            Self::View => "VIEW",
            Self::Files => "FILES",
            Self::Terminal => "TERMINAL",
        }
    }
}

/// Where a command's shortcut works: its GPUI key context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Any focus in any window (no key context).
    Global,
    Workspace,
    /// Inside the workspace while a chat is focused.
    Chat,
    /// Inside the workspace while a terminal is focused.
    Terminal,
    /// Inside the workspace while the file editor is focused.
    FileEditor,
    /// The Settings window.
    Preferences,
}
/// How two scopes can be active together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// Never at the same time: the same keystroke is fine.
    None,
    /// Both handle the keystroke at once; only one of them can win.
    Same,
    /// One is inside the other: the inner command wins while it has focus.
    Nested,
}
impl Scope {
    fn context(self) -> Option<&'static str> {
        match self {
            Self::Global => None,
            Self::Workspace => Some("Workspace"),
            Self::Chat => Some("Chat"),
            Self::Terminal => Some("Terminal"),
            Self::FileEditor => Some("FileEditor"),
            Self::Preferences => Some("Preferences"),
        }
    }
    fn inside_workspace(self) -> bool {
        matches!(self, Self::Chat | Self::Terminal | Self::FileEditor)
    }
    pub fn overlap(self, other: Self) -> Overlap {
        if self == other || self == Self::Global || other == Self::Global {
            Overlap::Same
        } else if (self == Self::Workspace && other.inside_workspace())
            || (other == Self::Workspace && self.inside_workspace())
        {
            Overlap::Nested
        } else {
            Overlap::None
        }
    }
    /// Completes "works …" in conflict messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "everywhere",
            Self::Workspace => "in the workspace",
            Self::Chat => "in a focused chat",
            Self::Terminal => "in a focused terminal",
            Self::FileEditor => "in the file editor",
            Self::Preferences => "in Settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandId {
    Quit,
    OpenSettings,
    OpenProject,
    CloseProject,
    SearchSettings,
    NewChat,
    NewChatIn,
    NewChatWithoutProject,
    StopChat,
    AllowRequest,
    DenyRequest,
    PreviousMessage,
    NextMessage,
    FindInChat,
    FindNext,
    FindPrevious,
    NewTab,
    CloseTab,
    FirstTab,
    SecondTab,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ToggleSidebar,
    ToggleTools,
    ShowTabs,
    ShowFiles,
    ShowChanges,
    ShowTasks,
    ShowAgent,
    GoToFile,
    SaveFile,
    SaveInEditor,
    CopyTerminal,
    PasteTerminal,
}

pub struct Command {
    pub id: CommandId,
    /// Stable identifier in `keyboard.shortcuts`; never rename.
    pub key: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub scope: Scope,
    action: fn() -> Box<dyn Action>,
    /// Default keystrokes in GPUI syntax. The first one is shown in tooltips.
    mac: &'static [&'static str],
    windows: &'static [&'static str],
    /// `None` uses the macOS defaults (Cmd is Super there).
    linux: Option<&'static [&'static str]>,
}

macro_rules! command {
    ($id:ident, $key:literal, $name:literal, $group:ident, $scope:ident, $action:expr,
     mac: $mac:expr, windows: $windows:expr $(, linux: $linux:expr)?) => {
        Command {
            id: CommandId::$id,
            key: $key,
            name: $name,
            group: Group::$group,
            scope: Scope::$scope,
            action: || Box::new($action),
            mac: &$mac,
            windows: &$windows,
            linux: command!(@linux $($linux)?),
        }
    };
    (@linux) => { None };
    (@linux $linux:expr) => { Some(&$linux) };
}

use super::chat::{
    AllowPending, DenyPending, FindInChat, FindNext, FindPrevious, NextMessage, PreviousMessage,
    StopChat,
};
use super::terminal::{CopyTerminal, PasteTerminal};
use super::{
    ClosePane, CloseProject, CloseTab, CodexTab, NewChat, NewChatIn, NewChatNoProject, NewTab,
    OpenFolder, QuickOpen, SaveFile, ShellTab, ShowChanges, ShowFiles, ShowPreferences,
    ShowSession, ShowTasks, ShowTerminals, SplitHorizontal, SplitVertical, ToggleInspector,
    ToggleSidebar,
};

/// Windows keeps unshifted Ctrl+C/D/P/S/W/Z for the terminal (docs/windows.md),
/// so workspace commands there use Ctrl+Shift.
pub const COMMANDS: &[Command] = &[
    command!(Quit, "app.quit", "Quit Canopy", App, Global, crate::app::Quit,
        mac: ["cmd-q"], windows: [], linux: []),
    command!(OpenSettings, "app.settings", "Open Settings", App, Workspace, ShowPreferences,
        mac: ["cmd-,"], windows: ["ctrl-,"]),
    command!(OpenProject, "app.open_project", "Open project…", App, Workspace, OpenFolder,
        mac: ["cmd-o"], windows: ["ctrl-shift-o"]),
    command!(CloseProject, "app.close_project", "Close project", App, Workspace, CloseProject,
        mac: ["cmd-shift-w"], windows: ["ctrl-shift-f4"]),
    command!(SearchSettings, "settings.search", "Search settings", App, Preferences,
        super::preferences::FocusSearch,
        mac: ["cmd-k"], windows: ["ctrl-k"], linux: ["ctrl-k"]),
    command!(NewChat, "chat.new", "New chat", Chat, Workspace, NewChat,
        mac: ["cmd-n"], windows: ["ctrl-shift-n"], linux: ["cmd-n", "ctrl-n"]),
    command!(NewChatIn, "chat.new_in", "New chat in…", Chat, Workspace, NewChatIn,
        mac: ["cmd-shift-n"], windows: ["ctrl-alt-n"], linux: ["cmd-shift-n", "ctrl-shift-n"]),
    command!(NewChatWithoutProject, "chat.new_without_project", "New chat without a project",
        Chat, Workspace, NewChatNoProject,
        mac: ["cmd-alt-n"], windows: ["ctrl-alt-shift-n"], linux: ["cmd-alt-n", "ctrl-alt-n"]),
    command!(StopChat, "chat.stop", "Stop the agent", Chat, Chat, StopChat,
        mac: ["cmd-."], windows: ["cmd-."]),
    // From the composer, cmd-enter arrives as the input's secondary Enter instead.
    command!(AllowRequest, "chat.allow", "Allow request or send now", Chat, Chat, AllowPending,
        mac: ["cmd-enter"], windows: ["cmd-enter"]),
    command!(DenyRequest, "chat.deny", "Deny request", Chat, Chat, DenyPending,
        mac: ["escape"], windows: ["escape"]),
    command!(PreviousMessage, "chat.previous_message", "Previous message", Chat, Chat,
        PreviousMessage, mac: ["alt-cmd-up"], windows: ["alt-cmd-up"]),
    command!(NextMessage, "chat.next_message", "Next message", Chat, Chat, NextMessage,
        mac: ["alt-cmd-down"], windows: ["alt-cmd-down"]),
    command!(FindInChat, "chat.find", "Find in chat", Chat, Chat, FindInChat,
        mac: ["cmd-f"], windows: ["cmd-f"]),
    command!(FindNext, "chat.find_next", "Find next", Chat, Chat, FindNext,
        mac: ["cmd-g"], windows: ["cmd-g"]),
    command!(FindPrevious, "chat.find_previous", "Find previous", Chat, Chat, FindPrevious,
        mac: ["cmd-shift-g"], windows: ["cmd-shift-g"]),
    command!(NewTab, "tabs.new", "New tab", TabsAndPanes, Workspace, NewTab,
        mac: ["cmd-t"], windows: ["ctrl-shift-t"]),
    command!(CloseTab, "tabs.close", "Close tab", TabsAndPanes, Workspace, CloseTab,
        mac: ["cmd-w"], windows: ["ctrl-shift-w"]),
    command!(FirstTab, "tabs.first", "Go to first tab", TabsAndPanes, Workspace, ShellTab,
        mac: ["cmd-1"], windows: ["ctrl-shift-1"]),
    command!(SecondTab, "tabs.second", "Go to second tab", TabsAndPanes, Workspace, CodexTab,
        mac: ["cmd-2"], windows: ["ctrl-shift-2"]),
    command!(SplitHorizontal, "panes.split_horizontal", "Split pane horizontally", TabsAndPanes,
        Workspace, SplitHorizontal, mac: ["cmd-d"], windows: ["ctrl-shift-d"]),
    command!(SplitVertical, "panes.split_vertical", "Split pane vertically", TabsAndPanes,
        Workspace, SplitVertical, mac: ["cmd-shift-d"], windows: ["ctrl-shift-e"]),
    command!(ClosePane, "panes.close", "Close pane", TabsAndPanes, Workspace, ClosePane,
        mac: ["cmd-alt-w"], windows: ["ctrl-shift-x"]),
    command!(ToggleSidebar, "view.sidebar", "Show or hide sidebar", View, Workspace,
        ToggleSidebar,
        mac: ["cmd-alt-b"], windows: ["ctrl-shift-b"], linux: ["cmd-alt-b", "ctrl-alt-b"]),
    command!(ToggleTools, "view.tools", "Show or hide tools", View, Workspace, ToggleInspector,
        mac: ["cmd-b"], windows: ["ctrl-shift-i"]),
    command!(ShowTabs, "view.tabs", "Show all tabs", View, Workspace, ShowTerminals,
        mac: ["cmd-alt-1"], windows: ["ctrl-alt-1"], linux: ["ctrl-alt-1"]),
    command!(ShowFiles, "view.files", "Show Files", View, Workspace, ShowFiles,
        mac: ["cmd-alt-2"], windows: ["ctrl-alt-2"], linux: ["ctrl-alt-2"]),
    command!(ShowChanges, "view.changes", "Show Changes", View, Workspace, ShowChanges,
        mac: ["cmd-alt-3", "cmd-shift-g"], windows: ["ctrl-alt-3", "ctrl-shift-g"],
        linux: ["ctrl-alt-3", "cmd-shift-g"]),
    command!(ShowTasks, "view.tasks", "Show Tasks", View, Workspace, ShowTasks,
        mac: ["cmd-alt-4"], windows: ["ctrl-alt-4"], linux: ["ctrl-alt-4"]),
    command!(ShowAgent, "view.agent", "Show Agent", View, Workspace, ShowSession,
        mac: ["cmd-alt-5", "cmd-shift-s"], windows: ["ctrl-alt-5", "ctrl-shift-a"],
        linux: ["ctrl-alt-5", "cmd-shift-s"]),
    command!(GoToFile, "files.go_to_file", "Go to file…", Files, Workspace, QuickOpen,
        mac: ["cmd-p"], windows: ["ctrl-shift-p"]),
    command!(SaveFile, "files.save", "Save file", Files, Workspace, SaveFile,
        mac: ["cmd-s"], windows: ["ctrl-shift-s"]),
    // Narrower than Save file: only while the editor has focus, so Windows can
    // use unshifted Ctrl+S without taking it from the terminal.
    command!(SaveInEditor, "files.save_in_editor", "Save file in the editor", Files,
        FileEditor, super::editor::SaveEditor, mac: ["cmd-s"], windows: ["ctrl-s"]),
    command!(CopyTerminal, "terminal.copy", "Copy", Terminal, Terminal, CopyTerminal,
        mac: ["cmd-c"], windows: ["ctrl-shift-c"]),
    command!(PasteTerminal, "terminal.paste", "Paste", Terminal, Terminal, PasteTerminal,
        mac: ["cmd-v"], windows: ["ctrl-shift-v"]),
];

impl CommandId {
    pub fn command(self) -> &'static Command {
        COMMANDS
            .iter()
            .find(|command| command.id == self)
            .expect("every command is in COMMANDS")
    }
}

/// Keys owned by a component's own interaction; not customizable.
fn fixed_bindings() -> Vec<KeyBinding> {
    use super::terminal::{TerminalBacktab, TerminalShiftEnter, TerminalTab};
    let mut bindings = vec![
        KeyBinding::new("escape", super::DismissNavigation, Some("Workspace")),
        KeyBinding::new("escape", super::chat::CloseFind, Some("ChatFind")),
        KeyBinding::new("tab", TerminalTab, Some("Terminal")),
        KeyBinding::new("shift-tab", TerminalBacktab, Some("Terminal")),
        KeyBinding::new("shift-enter", TerminalShiftEnter, Some("Terminal")),
    ];
    // ⌘1…⌘9 pick a model while the picker is open, whatever they are bound to.
    bindings.extend(
        (1..=9).map(|n| KeyBinding::new(&format!("cmd-{n}"), NoAction, Some("ModelPicker"))),
    );
    bindings
}

impl Command {
    /// Canonical default keystrokes.
    pub fn defaults(&self, platform: Platform) -> Vec<String> {
        let raw = match platform {
            Platform::Mac => self.mac,
            Platform::Windows => self.windows,
            Platform::Linux => self.linux.unwrap_or(self.mac),
        };
        canonical_list(raw.iter().copied()).unwrap_or_default()
    }
    /// The keystrokes in effect: the user's, or the defaults when there are none
    /// (or the stored ones cannot be used by this build).
    pub fn keystrokes(&self, platform: Platform, overrides: &ShortcutOverrides) -> Vec<String> {
        overrides
            .get(self.key)
            .and_then(|stored| canonical_list(stored.iter().map(String::as_str)))
            .unwrap_or_else(|| self.defaults(platform))
    }
    pub fn modified(&self, platform: Platform, overrides: &ShortcutOverrides) -> bool {
        self.keystrokes(platform, overrides) != self.defaults(platform)
    }
}

/// GPUI's own spelling of a keystroke (`secondary-k` → `cmd-k` on macOS), so
/// equal shortcuts compare equal.
pub fn canonical(keystroke: &str) -> Option<String> {
    let parsed = Keystroke::parse(keystroke).ok()?;
    (!parsed.key.is_empty()).then(|| parsed.unparse())
}
fn canonical_list<'a>(keystrokes: impl Iterator<Item = &'a str>) -> Option<Vec<String>> {
    let mut list: Vec<String> = Vec::new();
    for keystroke in keystrokes {
        let keystroke = canonical(keystroke)?;
        if !list.contains(&keystroke) {
            list.push(keystroke);
        }
    }
    Some(list)
}

/// `overrides` with `keystrokes` for `command`; the defaults are not stored.
pub fn with_keystrokes(
    overrides: &ShortcutOverrides,
    platform: Platform,
    command: &Command,
    keystrokes: Vec<String>,
) -> ShortcutOverrides {
    let mut next = overrides.clone();
    if keystrokes == command.defaults(platform) {
        next.remove(command.key);
    } else {
        next.set(command.key, keystrokes);
    }
    next
}

/// Assigns `keystroke` as the only shortcut of `command`; the commands in
/// `take_from` lose it (reassigning a conflict).
pub fn assign(
    overrides: &ShortcutOverrides,
    platform: Platform,
    command: &Command,
    keystroke: &str,
    take_from: &[CommandId],
) -> ShortcutOverrides {
    let mut next = with_keystrokes(overrides, platform, command, vec![keystroke.to_owned()]);
    for other in take_from {
        let other = other.command();
        let mut keys = other.keystrokes(platform, &next);
        keys.retain(|k| k != keystroke);
        next = with_keystrokes(&next, platform, other, keys);
    }
    next
}

pub struct Conflict {
    pub command: &'static Command,
    pub overlap: Overlap,
}
/// Other commands whose shortcut `keystroke` already is, where both can apply.
pub fn conflicts(
    overrides: &ShortcutOverrides,
    platform: Platform,
    command: &Command,
    keystroke: &str,
) -> Vec<Conflict> {
    COMMANDS
        .iter()
        .filter(|other| other.id != command.id)
        .filter_map(|other| {
            let overlap = command.scope.overlap(other.scope);
            (overlap != Overlap::None
                && other
                    .keystrokes(platform, overrides)
                    .iter()
                    .any(|k| k == keystroke))
            .then_some(Conflict {
                command: other,
                overlap,
            })
        })
        .collect()
}

/// Whether a recorded keystroke can be a shortcut: it needs Ctrl, Alt or the
/// platform key (a plain or Shift-only key would be typed text), unless it is a
/// function key.
pub fn recordable(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    let function_key = keystroke
        .key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    !matches!(
        keystroke.key.as_str(),
        "" | "shift" | "control" | "alt" | "platform" | "function"
    ) && (m.control || m.alt || m.platform || function_key)
}
pub fn is_modifier_only(keystroke: &Keystroke) -> bool {
    matches!(
        keystroke.key.as_str(),
        "shift" | "control" | "alt" | "platform" | "function"
    )
}

/// Keycaps in platform order: `⌃ ⌥ ⇧ ⌘ K` or `Ctrl Alt Shift Win K`.
pub fn keycaps(keystroke: &str) -> Vec<SharedString> {
    let Ok(parsed) = Keystroke::parse(keystroke) else {
        return vec![keystroke.to_owned().into()];
    };
    let mac = cfg!(target_os = "macos");
    let m = parsed.modifiers;
    let mut caps: Vec<SharedString> = [
        (m.control, "⌃", "Ctrl"),
        (m.alt, "⌥", "Alt"),
        (m.shift, "⇧", "Shift"),
        (m.platform, "⌘", "Win"),
    ]
    .into_iter()
    .filter(|(on, ..)| *on)
    .map(|(_, glyph, word)| SharedString::from(if mac { glyph } else { word }))
    .collect();
    caps.push(
        Kbd::format(&Keystroke {
            modifiers: Modifiers::none(),
            key: parsed.key,
            key_char: None,
        })
        .into(),
    );
    caps
}
/// One-line text for tooltips and hints: `⇧⌘D` or `Ctrl+Shift+D`.
pub fn label(keystroke: &str) -> String {
    Keystroke::parse(keystroke)
        .map(|k| Kbd::format(&k))
        .unwrap_or_else(|_| keystroke.to_owned())
}

/// The command keymap and the user's overrides, owned app-wide.
pub struct Keymap {
    platform: Platform,
    /// Overrides as stored in settings.
    persisted: ShortcutOverrides,
    /// An edit already bound but not committed by the settings owner yet.
    pending: Option<ShortcutOverrides>,
    menus: Option<fn() -> Vec<Menu>>,
    _settings: Option<Subscription>,
}
impl Global for Keymap {}
impl Keymap {
    pub fn platform(&self) -> Platform {
        self.platform
    }
    /// Overrides in effect, including an edit not yet saved.
    pub fn overrides(&self) -> &ShortcutOverrides {
        self.pending.as_ref().unwrap_or(&self.persisted)
    }
    pub fn keystrokes(&self, id: CommandId) -> Vec<String> {
        id.command().keystrokes(self.platform, self.overrides())
    }
}

/// Binds every command once per app; later calls only attach the settings
/// owner when it exists. Views that need the bindings call it from `new`.
pub fn init(cx: &mut App) {
    init_for(Platform::current(), cx);
}
pub(crate) fn init_for(platform: Platform, cx: &mut App) {
    if !cx.has_global::<Keymap>() {
        cx.set_global(Keymap {
            platform,
            persisted: ShortcutOverrides::default(),
            pending: None,
            menus: None,
            _settings: None,
        });
        cx.bind_keys(fixed_bindings());
        rebind(cx);
    }
    if cx.global::<Keymap>()._settings.is_none()
        && let Some(settings) = cx.try_global::<AppState>().map(|app| app.settings.clone())
    {
        let subscription = cx.observe(&settings, |_, cx| sync(cx));
        cx.global_mut::<Keymap>()._settings = Some(subscription);
        sync(cx);
    }
}

/// Builds the menu bar now and again after every rebind: macOS menus copy their
/// key equivalents from the keymap when they are set.
pub fn set_menus(menus: fn() -> Vec<Menu>, cx: &mut App) {
    init(cx);
    cx.global_mut::<Keymap>().menus = Some(menus);
    cx.set_menus(menus());
}

/// Binds and saves new overrides. They apply at once; the settings owner writes
/// them in the background.
pub fn set_overrides(overrides: ShortcutOverrides, cx: &mut App) {
    init(cx);
    cx.global_mut::<Keymap>().pending = Some(overrides.clone());
    rebind(cx);
    if let Some(settings) = cx.try_global::<AppState>().map(|app| app.settings.clone()) {
        settings.update(cx, |settings, cx| {
            settings.remember(vec![Change::Shortcuts(overrides)], cx)
        });
    }
}

/// The shown shortcut of a command (its first keystroke), if it has one.
pub fn shortcut(id: CommandId, cx: &App) -> Option<String> {
    let keystrokes = match cx.try_global::<Keymap>() {
        Some(keymap) => keymap.keystrokes(id),
        None => id.command().defaults(Platform::current()),
    };
    keystrokes.first().map(|k| label(k))
}
/// `"{text} ({shortcut})"`, or just `text` when the command has no shortcut.
pub fn with_shortcut(text: impl Into<String>, id: CommandId, cx: &App) -> String {
    let text = text.into();
    match shortcut(id, cx) {
        Some(shortcut) => format!("{text} ({shortcut})"),
        None => text,
    }
}

fn bindings(keymap: &Keymap) -> Vec<KeyBinding> {
    let overrides = keymap.overrides();
    let mut bindings = Vec::new();
    for command in COMMANDS {
        let predicate = command.scope.context().map(|context| {
            std::rc::Rc::new(KeyBindingContextPredicate::parse(context).expect("valid context"))
        });
        // The first keystroke is shown, and GPUI shows the last binding added.
        for keystroke in command.keystrokes(keymap.platform, overrides).iter().rev() {
            // Canonical keystrokes always parse.
            if let Ok(binding) = KeyBinding::load(
                keystroke,
                (command.action)(),
                predicate.clone(),
                false,
                None,
                &DummyKeyboardMapper,
            ) {
                bindings.push(binding.with_meta(OURS));
            }
        }
    }
    bindings
}

/// Replaces our bindings and keeps everyone else's, in their order.
fn rebind(cx: &mut App) {
    let keymap = cx.global::<Keymap>();
    let ours = bindings(keymap);
    let menus = keymap.menus;
    let retained: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|binding| binding.meta() != Some(OURS))
        .cloned()
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(retained.into_iter().chain(ours));
    if let Some(menus) = menus {
        cx.set_menus(menus());
    }
}

/// Follows the stored overrides. A pending edit stays bound until the settings
/// owner has committed it, or drops when its write failed.
fn sync(cx: &mut App) {
    let Some(app) = cx.try_global::<AppState>() else {
        return;
    };
    let settings = app.settings.read(cx);
    if !settings.ready {
        return;
    }
    let stored = settings.values.shortcuts.clone();
    let failed = !settings.saving && settings.error.is_some();
    let keymap = cx.global::<Keymap>();
    let before = keymap.overrides().clone();
    let pending = keymap
        .pending
        .clone()
        .filter(|pending| *pending != stored && !failed);
    let keymap = cx.global_mut::<Keymap>();
    keymap.persisted = stored;
    keymap.pending = pending;
    if *keymap.overrides() != before {
        rebind(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};

    const PLATFORMS: [Platform; 3] = [Platform::Mac, Platform::Windows, Platform::Linux];

    fn first(id: CommandId) -> String {
        id.command().defaults(Platform::current())[0].clone()
    }

    #[test]
    fn defaults_are_valid_unique_and_free_of_conflicts_on_every_platform() {
        for (i, a) in COMMANDS.iter().enumerate() {
            for b in &COMMANDS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.key, b.key);
            }
        }
        for platform in PLATFORMS {
            for command in COMMANDS {
                let raw = match platform {
                    Platform::Mac => command.mac,
                    Platform::Windows => command.windows,
                    Platform::Linux => command.linux.unwrap_or(command.mac),
                };
                assert_eq!(
                    command.defaults(platform).len(),
                    raw.len(),
                    "{} on {platform:?}",
                    command.key
                );
                for keystroke in command.defaults(platform) {
                    let same =
                        conflicts(&ShortcutOverrides::default(), platform, command, &keystroke)
                            .into_iter()
                            .filter(|c| c.overlap == Overlap::Same)
                            .map(|c| c.command.key)
                            .collect::<Vec<_>>();
                    assert!(same.is_empty(), "{} {keystroke} {same:?}", command.key);
                    // The terminal keeps unshifted Ctrl+C/D/P/S/W/Z on Windows.
                    if platform == Platform::Windows
                        && matches!(command.scope, Scope::Workspace | Scope::Global)
                    {
                        let parsed = Keystroke::parse(&keystroke).unwrap();
                        assert!(
                            !parsed.modifiers.control
                                || parsed.modifiers.shift
                                || parsed.modifiers.alt
                                || !["c", "d", "p", "s", "w", "z"].contains(&parsed.key.as_str()),
                            "{} takes {keystroke} from the terminal",
                            command.key
                        );
                    }
                }
            }
        }
        let new_chat = CommandId::NewChat.command();
        assert_eq!(
            new_chat.defaults(Platform::Mac),
            [canonical("cmd-n").unwrap()]
        );
        assert_eq!(new_chat.defaults(Platform::Windows), ["ctrl-shift-n"]);
        assert!(
            CommandId::Quit
                .command()
                .defaults(Platform::Windows)
                .is_empty()
        );
    }

    #[test]
    fn conflicts_reassignment_and_reset() {
        let platform = Platform::current();
        let none = ShortcutOverrides::default();
        let new_tab = CommandId::NewTab.command();
        let go_to_file = first(CommandId::GoToFile);
        let same = conflicts(&none, platform, new_tab, &go_to_file);
        assert_eq!(same.len(), 1);
        assert_eq!(same[0].command.id, CommandId::GoToFile);
        assert_eq!(same[0].overlap, Overlap::Same);
        let nested = conflicts(&none, platform, new_tab, &first(CommandId::FindInChat));
        assert_eq!(nested[0].overlap, Overlap::Nested);
        // A chat and a terminal are never focused together.
        let copy = first(CommandId::CopyTerminal);
        assert!(conflicts(&none, platform, CommandId::FindInChat.command(), &copy).is_empty());

        let moved = assign(
            &none,
            platform,
            new_tab,
            &go_to_file,
            &[CommandId::GoToFile],
        );
        assert_eq!(
            new_tab.keystrokes(platform, &moved),
            std::slice::from_ref(&go_to_file)
        );
        assert!(
            CommandId::GoToFile
                .command()
                .keystrokes(platform, &moved)
                .is_empty()
        );
        assert_eq!(moved.get("files.go_to_file"), Some(&[][..]));
        assert!(new_tab.modified(platform, &moved));

        // Taking one of two defaults leaves the other.
        let changes = CommandId::ShowChanges.command();
        let both = changes.defaults(platform);
        let taken = assign(
            &none,
            platform,
            new_tab,
            &both[0],
            &[CommandId::ShowChanges],
        );
        assert_eq!(changes.keystrokes(platform, &taken), both[1..]);

        let reset = with_keystrokes(&moved, platform, new_tab, new_tab.defaults(platform));
        assert!(reset.get("tabs.new").is_none());
        assert!(!new_tab.modified(platform, &reset));
        // A stored value this build cannot parse falls back to the default.
        let mut broken = ShortcutOverrides::default();
        broken.set("tabs.new", vec!["cmd-nosuchkey-x".into()]);
        assert_eq!(
            new_tab.keystrokes(platform, &broken),
            new_tab.defaults(platform)
        );
    }

    #[test]
    fn only_modified_keys_or_function_keys_can_be_recorded() {
        let parse = |s| Keystroke::parse(s).unwrap();
        for accepted in ["cmd-k", "ctrl-shift-1", "alt-x", "f5", "shift-f12"] {
            assert!(recordable(&parse(accepted)), "{accepted}");
        }
        for rejected in ["a", "shift-a", "enter", "space"] {
            assert!(!recordable(&parse(rejected)), "{rejected}");
        }
        assert!(is_modifier_only(&parse("shift")));
    }

    struct Probe {
        focus: FocusHandle,
        hits: Vec<&'static str>,
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .key_context("Workspace")
                .track_focus(&self.focus)
                .on_action(cx.listener(|this, _: &NewChat, _, _| this.hits.push("new chat")))
                .on_action(cx.listener(|this, _: &NewTab, _, _| this.hits.push("new tab")))
                .size_full()
        }
    }
    fn other_bindings(cx: &App) -> usize {
        cx.key_bindings()
            .borrow()
            .bindings()
            .filter(|b| b.meta() != Some(OURS))
            .count()
    }

    #[gpui_kit::test]
    fn overrides_rebind_real_dispatch_and_keep_component_bindings(cx: &mut TestAppContext) {
        let (window, components) = cx.update(|cx| {
            gpui_kit::init(cx);
            let components = other_bindings(cx);
            init(cx);
            let window = cx
                .open_window(Default::default(), |_, cx| {
                    cx.new(|cx| Probe {
                        focus: cx.focus_handle(),
                        hits: vec![],
                    })
                })
                .unwrap();
            (window, components)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let probe = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            let focus = probe.read(cx).focus.clone();
            focus.focus(window, cx)
        });
        let new_chat = first(CommandId::NewChat);
        cx.simulate_keystrokes(&new_chat);
        assert_eq!(probe.read_with(&cx, |p, _| p.hits.clone()), ["new chat"]);

        let custom = canonical("ctrl-alt-k").unwrap();
        cx.update(|_, cx| {
            let platform = cx.global::<Keymap>().platform();
            let next = assign(
                cx.global::<Keymap>().overrides(),
                platform,
                CommandId::NewChat.command(),
                &custom,
                &[],
            );
            set_overrides(next, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes(&new_chat);
        cx.simulate_keystrokes(&custom);
        cx.simulate_keystrokes(&first(CommandId::NewTab));
        assert_eq!(
            probe.read_with(&cx, |p, _| p.hits.clone()),
            ["new chat", "new chat", "new tab"]
        );
        cx.update(|_, cx| {
            assert_eq!(shortcut(CommandId::NewChat, cx), Some(label(&custom)));
            // gpui-component's own bindings (inputs, menus, …) and the fixed keys stay;
            // only ours were replaced.
            assert_eq!(other_bindings(cx), components + fixed_bindings().len());
        });
    }

    #[gpui_kit::test]
    fn stored_overrides_apply_and_an_unsaved_edit_stays_until_committed(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            AppState::install_test(canopy_desktop::state::workspace::Workspace::new(), cx);
            init(cx);
        });
        let settings = cx.update(|cx| cx.global::<AppState>().settings.clone());
        let platform = Platform::current();
        let custom = canonical("ctrl-alt-k").unwrap();
        let mut stored = ShortcutOverrides::default();
        stored.set("tabs.new", vec![custom.clone()]);
        settings.update(cx, |settings, cx| {
            settings.values.shortcuts = stored.clone();
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(
                cx.global::<Keymap>().keystrokes(CommandId::NewTab),
                std::slice::from_ref(&custom)
            )
        });

        // Not written yet (the test owner has no storage): an unrelated settings
        // change keeps the edit bound.
        let edit = with_keystrokes(&stored, platform, CommandId::NewTab.command(), vec![]);
        cx.update(|cx| set_overrides(edit.clone(), cx));
        settings.update(cx, |settings, cx| {
            settings.values.reopen_last_workspace = false;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(
                cx.global::<Keymap>()
                    .keystrokes(CommandId::NewTab)
                    .is_empty()
            )
        });
        // Committed: the stored value takes over and nothing stays pending.
        settings.update(cx, |settings, cx| {
            settings.values.shortcuts = edit.clone();
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let keymap = cx.global::<Keymap>();
            assert!(keymap.pending.is_none());
            assert_eq!(keymap.overrides(), &edit);
        });
    }
}
