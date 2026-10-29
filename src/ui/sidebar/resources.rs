//! Footer readout of Canopy's CPU and memory (General → Show CPU and RAM usage).
//! `ResourceUsage` samples; this only shows its latest sample.
use crate::app_state::AppState;
use crate::ui::components::hover_card::content as card;
use crate::ui::{components::*, theme as t};
use canopy_desktop::platform::resources::{Usage, format_cpu, format_memory};
use gpui_kit::base::Placement;
use gpui_kit::component::Icon;
use gpui_kit::*;
use std::sync::Arc;

/// Figures keep their width as they change.
fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// `3% · 412 MB`, each figure in a slot that doesn't shrink as values change.
fn figures(usage: Option<Usage>) -> Div {
    let (cpu, memory) = usage.map_or(("–".into(), "–".into()), |usage| {
        (format_cpu(usage.cpu), format_memory(usage.memory))
    });
    row()
        .flex_shrink_0()
        .gap_1()
        .font_features(tabular())
        .child(row().justify_end().min_w(rems(2.)).child(cpu))
        .child(div().text_color(t::faint()).child("·"))
        .child(row().justify_end().min_w(rems(3.25)).child(memory))
}

/// `None` while the setting is off.
pub(super) fn readout(cx: &App) -> Option<impl IntoElement> {
    let resources = cx.global::<AppState>().resources.read(cx);
    if !resources.enabled() {
        return None;
    }
    let total = resources.sample.map(|sample| sample.total());
    Some(
        row()
            .id("resource-usage")
            .flex_shrink_0()
            .h_6()
            .px_1()
            .gap_1p5()
            .text_xs()
            .text_color(t::muted())
            .child(
                Icon::default()
                    .path("canopy/cpu.svg")
                    .size_3()
                    .text_color(t::faint()),
            )
            .child(figures(total))
            .hover_card("resource-usage", Placement::Top, |_, cx| {
                details(cx).into_any_element()
            }),
    )
}

/// Read when shown, so an open card follows each new sample.
fn details(cx: &App) -> Div {
    let sample = cx.global::<AppState>().resources.read(cx).sample;
    let started = match sample.map_or(0, |sample| sample.child_processes) {
        0 => "No agents or terminals running".to_owned(),
        1 => "1 process started by Canopy".to_owned(),
        count => format!("{count} processes started by Canopy"),
    };
    let line = |icon: &'static str, label: &'static str, usage: Option<Usage>| {
        card::fact(Icon::default().path(icon))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(figures(usage).text_color(t::muted()))
    };
    card::body()
        .child(
            column()
                .gap_0p5()
                .child(card::title("CPU and memory"))
                .child(div().text_xs().text_color(t::muted()).child(started)),
        )
        .child(card::separator())
        .child(
            card::facts()
                .child(line(
                    "canopy/monitor.svg",
                    "Canopy",
                    sample.map(|sample| sample.app),
                ))
                .child(line(
                    "canopy/square-terminal.svg",
                    "Agents and terminals",
                    sample.map(|sample| sample.children),
                ))
                .child(line(
                    "canopy/cpu.svg",
                    "Total",
                    sample.map(|sample| sample.total()),
                )),
        )
        .child(card::separator())
        .child(
            div()
                .text_xs()
                .line_height(t::chat_metadata_line_height())
                .text_color(t::faint())
                .child("Updated every second. 100% CPU is one full core."),
        )
}
