use super::super::theme as t;
use super::icon;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::*;

/// Secondary Canopy action. Width/margins and callbacks belong to the caller.
/// Native builder also supplies disabled/loading, keyboard focus and tooltips.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    button_base(id).label(label)
}

/// Compact removable value in a multi-select field. Trailing × lives in the label.
pub fn choice_chip(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    button(id, label)
        .h(px(22.))
        .min_h(px(22.))
        .max_h(px(22.))
        .px(px(6.))
        .text_size(px(11.))
}

/// Left-aligned list action with foreground-only hover, matching disclosure headers.
pub fn list_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    hovered: bool,
    cx: &App,
) -> Button {
    use gpui_kit::component::button::ButtonCustomVariant;
    let label = label.into();
    let foreground = if hovered { t::text() } else { t::secondary() };
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).foreground(foreground))
        .xsmall()
        .compact()
        .h(px(28.))
        .px(px(12.))
        .rounded(px(4.))
        .border_0()
        .cursor_pointer()
        .accessibility_label(label.clone())
        .child(
            div()
                .w_full()
                .min_w_0()
                .text_size(px(12.))
                .text_color(foreground)
                .text_left()
                .text_ellipsis()
                .child(label),
        )
}

fn button_base(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .xsmall()
        .compact()
        .h(px(28.))
        .px(px(12.))
        .rounded(px(4.))
        .border_1()
        .text_size(px(12.))
}

/// Compact, keyboard-accessible icon action with an explicit accessible name.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    label: impl Into<SharedString>,
) -> Button {
    icon_button_base(id, label).icon(icon(name))
}

fn icon_button_base(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    Button::new(id)
        .small()
        .ghost()
        .compact()
        .accessibility_label(label.clone())
        .tooltip(label)
        .size(px(20.))
        .min_w(px(20.))
        .max_w(px(20.))
        .min_h(px(20.))
        .max_h(px(20.))
        .p_0()
        .rounded(px(3.))
        .border_0()
        .text_color(t::muted())
}

pub fn loading_icon_button(
    id: impl Into<ElementId>,
    name: impl Into<gpui_kit::component::Icon>,
    label: impl Into<SharedString>,
    loading: Option<&super::ButtonLoading>,
) -> Button {
    let label = label.into();
    let button = icon_button_base(id, label.clone());
    if let Some(loading) = loading {
        button
            .accessibility_label(if loading.active() {
                format!("{label}, in progress").into()
            } else {
                label
            })
            .loading(loading.active())
            .child(loading.icon(name))
    } else {
        button.icon(name.into())
    }
}

/// Quiet toolbar action: foreground-only hover, no background fill.
pub fn quiet_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    hovered: bool,
    cx: &App,
) -> Button {
    use gpui_kit::component::button::ButtonCustomVariant;
    let label = label.into();
    Button::new(id)
        .custom(ButtonCustomVariant::new(cx).foreground(if hovered {
            t::text()
        } else {
            t::faint()
        }))
        .small()
        .compact()
        .h(px(20.))
        .p_0()
        .border_0()
        .cursor_pointer()
        .accessibility_label(label)
}

/// Accent action using the same dimensions and interaction API as button().
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    button_base(id).primary().label(label)
}

/// Async action with an owned, interruptible loader reveal. The label stays
/// stable so only the loader's slot changes the button's intrinsic width.
pub fn loading_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    loading: &super::ButtonLoading,
) -> Button {
    let label = label.into();
    button_base(id)
        .accessibility_label(if loading.active() {
            SharedString::from(format!("{label}, in progress"))
        } else {
            label.clone()
        })
        .loading(loading.active())
        .child(loading.label(label))
}

pub fn primary_loading_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    loading: &super::ButtonLoading,
) -> Button {
    loading_button(id, label, loading).primary()
}

/// Existing leading icons share their slot with the loader instead of adding
/// a second icon or changing the action's familiar presentation.
pub fn loading_button_with_icon(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl Into<gpui_kit::component::Icon>,
    loading: &super::ButtonLoading,
) -> Button {
    let label = label.into();
    button_base(id)
        .accessibility_label(if loading.active() {
            SharedString::from(format!("{label}, in progress"))
        } else {
            label.clone()
        })
        .loading(loading.active())
        .child(
            super::row()
                .min_w_0()
                .gap(px(t::SPACING_UNIT))
                .child(loading.icon(icon))
                .child(div().min_w_0().truncate().child(label)),
        )
}

/// Selection styling shared by tool and profile choices; callbacks belong to the caller.
pub fn selection_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Button {
    button(id, label).bg(if selected {
        t::selected()
    } else {
        rgba(0).into()
    })
}

/// Shared geometry and interaction for compact worktree actions.
fn worktree_action(id: impl Into<ElementId>, label: &'static str, cx: &App) -> Button {
    use gpui_kit::component::button::ButtonCustomVariant;
    let side = px(t::SPACING_UNIT * 4.);
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .hover(t::red().opacity(0.12))
                .active(t::red().opacity(0.2)),
        )
        .compact()
        .size(side)
        .min_w(side)
        .max_w(side)
        .min_h(side)
        .max_h(side)
        .flex_shrink_0()
        .rounded(px(3.))
        .p_0()
        .border_0()
        .accessibility_label(label)
        .tooltip(label)
}
pub fn stop_processes_button(id: impl Into<ElementId>, cx: &App) -> Button {
    worktree_action(id, "Stop all processes in this worktree", cx)
        .child(div().size(px(8.)).rounded(px(1.)).bg(t::red()))
}
pub fn loading_stop_button(
    id: impl Into<ElementId>,
    loading: &super::ButtonLoading,
    cx: &App,
) -> Button {
    worktree_action(id, "Stop all processes in this worktree", cx)
        .loading(loading.active())
        .accessibility_label(if loading.active() {
            "Stop all processes in this worktree, in progress"
        } else {
            "Stop all processes in this worktree"
        })
        .text_color(t::red())
        .child(loading.visual(div().size(px(8.)).rounded(px(1.)).bg(t::red())))
}
pub fn remove_worktree_button(id: impl Into<ElementId>, cx: &App) -> Button {
    worktree_action(id, "Remove worktree", cx).child(icon(IconName::Close).size(px(12.)))
}

pub fn close_tab_button(id: impl Into<ElementId>, cx: &App) -> Button {
    worktree_action(id, "Close tab", cx).child(icon(IconName::Close).size(px(12.)))
}
