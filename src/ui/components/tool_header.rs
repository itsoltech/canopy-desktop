//! Hover wrapper for managed button tooltips.
use gpui_kit::{component::button::Button, *};
/// Separate hitbox prevents GPUI Kit's managed tooltip from replacing the hover callback.
/// Set dimensions/margins on the returned native Styled builder at the call site.
pub fn hover_action(
    id: impl Into<ElementId>,
    action: Button,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div().id(id).on_hover(on_hover).child(action)
}
