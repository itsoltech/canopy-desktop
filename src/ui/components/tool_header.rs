//! Tool disclosure presentation. Click policy and hover state belong to the sidebar.
use super::super::theme as t;
use super::{badge, row, tool_icon};
use gpui_kit::{
    component::{
        Icon, IconName,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    },
    *,
};
pub struct ToolHeader<'a> {
    pub name: &'a str,
    pub expanded: Option<bool>,
    pub chevron: Option<Icon>,
    pub hovered: bool,
    pub running: usize,
    pub missing: bool,
}
impl ToolHeader<'_> {
    pub fn button(self, id: impl Into<ElementId>, cx: &App) -> Button {
        let foreground = if self.hovered {
            t::text()
        } else {
            t::secondary()
        };
        let label = self
            .expanded
            .map(|open| format!("{} {}", if open { "Collapse" } else { "Expand" }, self.name))
            .unwrap_or_else(|| self.name.to_owned());
        Button::new(id)
            .custom(ButtonCustomVariant::new(cx).foreground(foreground))
            .compact()
            .w_full()
            .h(px(t::ROW))
            .p_0()
            .border_0()
            .cursor_pointer()
            .accessibility_label(label)
            .child(
                row()
                    .w_full()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(foreground)
                    .children(self.chevron.map(|icon| icon.text_color(foreground)))
                    .child(tool_icon(self.name, IconName::SquareTerminal))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_left()
                            .text_ellipsis()
                            .child(self.name.to_owned()),
                    )
                    .children((self.running > 0).then(|| badge(self.running.to_string())))
                    .children(self.missing.then(|| {
                        div()
                            .text_color(t::muted())
                            .text_size(px(10.))
                            .child("missing")
                    })),
            )
    }
}
/// Separate hitbox prevents GPUI Kit's managed tooltip from replacing the hover callback.
/// Set dimensions/margins on the returned native Styled builder at the call site.
pub fn hover_action(
    id: impl Into<ElementId>,
    action: Button,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div().id(id).on_hover(on_hover).child(action)
}
