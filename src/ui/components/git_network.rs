//! Transport presentation. Callers retain operations, form entities and callbacks.
use super::{super::theme as t, *};
use canopy_desktop::git::network::Operation;
use gpui_kit::{
    component::{button::Button, input::InputState, select::SelectState},
    *,
};

pub fn transfer_button(operation: Operation, feedback: &ButtonLoading) -> Button {
    match operation {
        Operation::Pull => loading_button("pull-branch", "↓ Pull", feedback),
        Operation::Push => loading_button("push-branch", "↑ Push", feedback),
    }
}

pub fn transfer_controls(mut action: impl FnMut(Operation) -> Button) -> Div {
    row().gap(px(8.)).children(
        [Operation::Pull, Operation::Push]
            .into_iter()
            .map(|operation| action(operation).flex_1()),
    )
}

/// Existing dropdown/input builders, with their state owned by UpstreamDialog.
pub fn upstream_form(
    local_branch: &str,
    operation: Operation,
    remote: &Entity<SelectState<Vec<SelectOption>>>,
    branch: &Entity<InputState>,
    disabled: bool,
    same_name_action: impl IntoElement,
) -> Div {
    column().gap(px(t::SPACING_UNIT * 4.))
        .child(div().text_color(t::secondary()).child(format!("{local_branch} has no upstream. Choose its remote branch.")))
        .child(modal::modal_field("Remote", dropdown(remote).w_full().disabled(disabled), None))
        .child(modal::modal_field("Remote branch", input(branch).w_full().disabled(disabled), None))
        .child(same_name_action)
        .child(div().text_size(px(11.)).text_color(t::muted()).child(match operation {
            Operation::Push => "A missing remote branch will be created. Upstream is saved after a successful push.",
            Operation::Pull => "The remote branch must already exist. Pull uses fast-forward only.",
        }))
}
