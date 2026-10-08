//! Canopy UI building blocks. Native GPUI builders keep Styled overrides and events.
mod button_loading;
mod buttons;
pub use button_loading::ButtonLoading;
mod skeleton;
pub mod toast;
pub use skeleton::skeleton_bar;
mod controls;
pub mod file_tree;
pub mod files;
pub mod git;
pub mod git_network;
pub mod git_tracking;
pub mod history;
pub mod integrations;
pub mod media;
pub mod session;
mod settings;
pub mod task_detail;
pub mod tasks;
pub use buttons::*;
pub use controls::*;
pub use settings::*;

use super::theme as t;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::*;

pub fn row() -> Div {
    div().flex().items_center().min_w_0()
}
pub fn column() -> Div {
    div().flex().flex_col().min_h_0().min_w_0()
}
pub fn icon(name: IconName) -> Icon {
    Icon::new(name).size(px(14.)).text_color(t::muted())
}
pub fn dot() -> Div {
    div()
        .size(px(8.))
        .flex_shrink_0()
        .rounded_full()
        .bg(t::green())
}
pub fn badge(label: impl Into<SharedString>) -> Div {
    row()
        .justify_center()
        .px(px(4.))
        .h(px(16.))
        .rounded(px(3.))
        .bg(t::hover())
        .text_size(px(10.))
        .text_color(t::secondary())
        .child(label.into())
}

pub fn custom_icon(name: &'static str) -> Icon {
    Icon::default()
        .path(format!("canopy/{name}.svg"))
        .size(px(14.))
        .text_color(t::muted())
}
pub fn tool_icon(name: &str, fallback: IconName) -> AnyElement {
    match name {
        "Claude Code" => custom_icon("claude")
            .text_color(rgb(0xd97757))
            .into_any_element(),
        "Codex" => custom_icon("openai")
            .text_color(rgb(0x000000))
            .into_any_element(),
        "Gemini CLI" => img("canopy/gemini.svg").size(px(14.)).into_any_element(),
        "OpenCode" => custom_icon("code").into_any_element(),
        "Shell" | "Droid" => custom_icon("terminal").into_any_element(),
        "LazyGit" => custom_icon("git-branch")
            .text_color(rgb(0xf05033))
            .into_any_element(),
        _ => icon(fallback).into_any_element(),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod notch;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use notch::*;

mod disclosure;
pub use disclosure::Disclosure;

mod pane_divider;
pub use pane_divider::pane_divider;

mod tabs;
pub use tabs::*;
mod pane;
pub use pane::*;

pub mod modal;
pub use modal::{ModalDismissed, TextPrompt};

mod project;
pub use project::empty_state;

mod terminal;
pub use terminal::{process_status_bar, terminal_message, terminal_surface, terminal_viewport};

mod titlebar;
pub use titlebar::{
    titlebar_leading_inset, titlebar_options, titlebar_trailing_inset, window_titlebar,
    workspace_titlebar,
};

mod tool_header;
pub use tool_header::{ToolHeader, hover_action};
