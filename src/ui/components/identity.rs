//! Project and checkout identity marks shared by the sidebar, its cards and
//! the worktree overview.
use super::super::theme as t;
use gpui_kit::*;

/// A project's monogram on its identity hue, at the sidebar's size. Callers
/// may restyle the size, radius and text size.
pub fn project_tile(name: &str) -> Div {
    // Projects with an origin are named `owner/repository`: the repository
    // names the project, so `canopy-desktop` keeps "CD" and its hue.
    let name = name.rsplit('/').next().unwrap_or(name);
    let hue = t::project_hue(name);
    let initials: String = name
        .split(['-', '_', ' ', '.'])
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    div()
        .size(t::project_tile_size())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems(0.3125))
        .bg(t::project_tile(hue))
        .text_color(t::project_tile_text(hue))
        .text_size(rems(0.6875))
        .font_weight(FontWeight::BOLD)
        .child(initials)
}

/// The small outlined tag after a branch (`default`, `locked`).
pub fn checkout_tag(label: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .px_1()
        .rounded(rems(0.25))
        .border_1()
        .border_color(t::border())
        .text_size(rems(0.65625))
        .line_height(rems(0.9375))
        .text_color(t::faint())
        .child(label)
}
