use super::super::theme as t;
use gpui_kit::component::{
    checkbox::Checkbox,
    combobox::{Combobox, ComboboxState},
    input::{Input, InputState},
    select::{Select, SelectDelegate, SelectItem, SelectState},
};
use gpui_kit::*;

/// The caller creates and retains InputState; the view never recreates it.
/// Use the native state for passwords/masks and the builder for embedded search.
pub fn input(state: &Entity<InputState>) -> Input {
    Input::new(state)
        .h(px(28.))
        .text_size(px(13.))
        .rounded(px(4.))
}

/// Data-generic select, with caller-owned state and width.
pub fn dropdown<D>(state: &Entity<SelectState<D>>) -> Select<D>
where
    D: SelectDelegate + 'static,
    <D::Item as SelectItem>::Value: PartialEq + Clone,
{
    Select::new(state)
        .menu_width(px(200.))
        .text_size(px(13.))
        .h(px(34.))
        .rounded(px(6.))
        .bg(t::hover())
        .border_color(t::control_border())
}

/// Searchable single- or multi-select, with the same compact form styling as `dropdown`.
pub fn combobox<D>(state: &Entity<ComboboxState<D>>) -> Combobox<D>
where
    D: SelectDelegate + 'static,
    <D::Item as SelectItem>::Value: PartialEq + Clone,
{
    Combobox::new(state)
        .placeholder("Choose…")
        .search_placeholder("Search…")
        .text_size(px(13.))
        .h(px(34.))
        .rounded(px(6.))
        .bg(t::hover())
        .border_color(t::control_border())
}

pub fn checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    label: impl Into<SharedString>,
) -> Checkbox {
    Checkbox::new(id)
        .checked(checked)
        .accessibility_label(label)
}

/// Shared multiline control; the caller owns the editing state.
pub fn textarea(
    state: &Entity<gpui_kit::component::input::TextareaState>,
) -> gpui_kit::component::input::Textarea {
    gpui_kit::component::input::Textarea::new(state)
        .text_size(px(12.))
        .rounded(px(4.))
}

/// Stable select value independent of its visible label.
#[derive(Clone)]
pub struct SelectOption {
    pub value: SharedString,
    pub label: SharedString,
}
impl SelectOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}
impl SelectItem for SelectOption {
    type Value = SharedString;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &SharedString {
        &self.value
    }
}

/// Shared source editor styling, using the same caller-owned native input engine.
pub fn code_editor(
    state: &Entity<gpui_kit::component::input::EditorState>,
) -> gpui_kit::component::input::Editor {
    gpui_kit::component::input::Editor::new(state)
        .h(relative(1.))
        .w_full()
        .appearance(false)
        .bordered(false)
        .font_family(t::MONO)
        .text_size(px(13.))
}
