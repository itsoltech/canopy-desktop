//! Transport presentation. Callers retain operations, form entities and callbacks.
use super::{super::theme as t, *};
use canopy_desktop::git::network::Operation;
use gpui_kit::{
    component::{button::Button, input::InputState, select::SelectState},
    *,
};

/// Pull or Push; `pending` is how many commits it would bring, shown when any.
pub fn transfer_button(operation: Operation, feedback: &ButtonLoading, pending: usize) -> Button {
    let (id, label) = match operation {
        Operation::Pull => ("pull-branch", "↓ Pull"),
        Operation::Push => ("push-branch", "↑ Push"),
    };
    let label = if pending > 0 {
        format!("{label} {pending}")
    } else {
        label.to_owned()
    };
    loading_button(id, label, feedback).px(px(9.))
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
