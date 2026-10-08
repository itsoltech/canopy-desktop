//! Static placeholders; sizing and surrounding spacing belong to callers.
use super::super::theme as t;
use gpui_kit::*;
pub fn skeleton_bar() -> Div {
    div().rounded(px(3.)).bg(t::hover())
}
