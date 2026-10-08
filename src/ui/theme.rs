use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

// Original Electron tokens in OKLCH. Convert once at each token access without
// rounding channels or alpha to 8-bit values. GPUI receives sRGB Hsla.
fn oklch(l: f32, c: f32, h: f32, alpha: f32) -> Hsla {
    let angle = h.to_radians();
    let a = c * angle.cos();
    let b = c * angle.sin();
    let ll = (l + 0.39633778 * a + 0.21580376 * b).powi(3);
    let mm = (l - 0.105561346 * a - 0.06385417 * b).powi(3);
    let ss = (l - 0.08948418 * a - 1.2914855 * b).powi(3);
    let encode = |v: f32| {
        if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    };
    Rgba {
        r: encode(4.0767417 * ll - 3.3077116 * mm + 0.23096994 * ss).clamp(0., 1.),
        g: encode(-1.268438 * ll + 2.6097574 * mm - 0.3413194 * ss).clamp(0., 1.),
        b: encode(-0.0041960863 * ll - 0.7034186 * mm + 1.7076147 * ss).clamp(0., 1.),
        a: alpha,
    }
    .into()
}
pub fn bg() -> Hsla {
    oklch(0.235, 0., 0., 1.)
}
pub fn sidebar() -> Hsla {
    rgb(0x1b1b1b).into()
}
/// Neutral edge shadow, distinct from the sidebar without using pure black.
pub fn scroll_edge_shadow() -> Hsla {
    rgb(0x121212).into()
}
pub fn elevated() -> Hsla {
    oklch(0.289, 0., 0., 1.)
}
pub fn text() -> Hsla {
    oklch(0.907, 0., 0., 0.86)
}
pub fn secondary() -> Hsla {
    oklch(0.907, 0., 0., 0.6)
}
pub fn muted() -> Hsla {
    oklch(0.907, 0., 0., 0.4)
}
pub fn faint() -> Hsla {
    oklch(0.907, 0., 0., 0.25)
}
pub fn border() -> Hsla {
    oklch(1., 0., 0., 0.06)
}
pub fn control_border() -> Hsla {
    oklch(1., 0., 0., 0.12)
}
pub fn input_bg() -> Hsla {
    oklch(0., 0., 0., 0.3)
}
pub fn modal_overlay() -> Hsla {
    oklch(0., 0., 0., 0.4)
}
pub fn hover() -> Hsla {
    oklch(1., 0., 0., 0.06)
}
pub fn selected() -> Hsla {
    oklch(1., 0., 0., 0.08)
}
pub fn accent() -> Hsla {
    oklch(0.782, 0.115, 243.83, 1.)
}
pub fn green() -> Hsla {
    oklch(0.802, 0.168, 147.32, 1.)
}
pub fn red() -> Hsla {
    oklch(0.712, 0.181, 22.84, 1.)
}
pub fn yellow() -> Hsla {
    oklch(0.883, 0.165, 92.22, 1.)
}

pub const SPACING_UNIT: f32 = 4.;
pub const ROW: f32 = 28.;
pub const TABS: f32 = 32.;
pub const STATUS: f32 = 24.;
pub const MONO: &str = "JetBrains Mono";

/// Match Electron's terminal fallback chain, preferring single-cell Nerd glyphs.
pub fn terminal_font() -> Font {
    Font {
        family: MONO.into(),
        features: FontFeatures(std::sync::Arc::new(vec![
            ("calt".into(), 0),
            ("liga".into(), 0),
            ("dlig".into(), 0),
        ])),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "JetBrainsMono Nerd Font Mono".into(),
            "JetBrainsMono NFM".into(),
            "JetBrainsMono Nerd Font".into(),
            "JetBrainsMono NF".into(),
            "Symbols Nerd Font Mono".into(),
            "FiraCode Nerd Font".into(),
            "Menlo".into(),
        ])),
        ..Default::default()
    }
}

pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = super::platform_ui::interface_font().into();
    // Electron rem is 16px; body typography is set explicitly by each view.
    theme.font_size = px(16.);
    theme.mono_font_family = MONO.into();
    theme.colors.background = bg();
    theme.colors.foreground = text();
    theme.colors.muted_foreground = muted();
    theme.colors.border = border();
    theme.colors.primary = accent();
    theme.colors.primary_foreground = bg();
    theme.colors.input = control_border();
    theme.colors.muted = elevated();
    theme.colors.popover = bg();
    theme.colors.popover_foreground = text();
    theme.colors.accent = hover();
    theme.colors.accent_foreground = text();
    theme.colors.ring = accent().opacity(0.45);
    theme.colors.button = hover();
    theme.colors.button_foreground = secondary();
    theme.colors.button_hover = selected();
    theme.colors.button_active = selected();
    theme.colors.button_primary = accent();
    theme.colors.button_primary_foreground = bg();
    theme.colors.button_primary_hover = accent();
    theme.colors.button_primary_active = accent();
    editor_theme(theme);
    // Built-in panels use ThemeTokens while inputs also read ThemeColor.
    // Keep both projections aligned after applying Canopy's overrides.
    theme.tokens = (&theme.colors).into();
    theme.radius = px(4.);
    canopy_desktop::motion::apply_to_theme(theme);
    Theme::sync_base(cx);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_idle() -> Hsla {
    oklch(0.8, 0.182, 151.71, 1.)
}

/// Unified diff surfaces share Canopy's semantic Git colors.
pub fn diff_line_background(kind: char) -> Hsla {
    match kind {
        '+' => green().opacity(0.12),
        '-' => red().opacity(0.12),
        'H' => hover(),
        _ => bg(),
    }
}

/// Shared graph palette; lane identity is independent of its screen column.
pub fn history_lane_color(index: usize) -> Hsla {
    match index % 6 {
        0 => accent(),
        1 => green(),
        2 => yellow(),
        3 => oklch(0.782, 0.115, 305., 1.),
        4 => red(),
        _ => oklch(0.802, 0.115, 185., 1.),
    }
}

/// Native editor chrome and syntax share Canopy's semantic palette.
fn editor_theme(theme: &mut Theme) {
    theme.colors.caret = accent();
    theme.colors.selection = accent().opacity(0.22);
    let highlight = std::sync::Arc::make_mut(&mut theme.highlight_theme);
    highlight.name = "Canopy".into();
    let style = &mut highlight.style;
    style.editor_background = Some(bg());
    style.editor_foreground = Some(text());
    style.editor_gutter_background = Some(sidebar());
    style.editor_active_line = Some(hover());
    style.editor_line_number = Some(faint());
    style.editor_active_line_number = Some(secondary());
    style.editor_invisible = Some(faint());
    // GPUI Kit exposes ThemeStyle colors through its hex-based serde API only.
    // Token conversion remains full precision; serialization is the final syntax boundary.
    let mut syntax = serde_json::to_value(&style.syntax).expect("syntax theme serialization");
    for (name, value) in syntax.as_object_mut().expect("syntax palette") {
        let color = match name.as_str() {
            "comment" | "comment.doc" | "hint" | "predictive" => muted(),
            "keyword" | "function" | "constructor" | "title" | "tag" | "link_text" | "link_uri" => {
                accent()
            }
            "string"
            | "string.regex"
            | "string.special"
            | "string.special.symbol"
            | "text.literal"
            | "text.code.span" => green().opacity(0.85),
            "number" | "boolean" | "constant" | "string.escape" => yellow().opacity(0.8),
            "type" | "enum" | "attribute" => accent().opacity(0.8),
            "punctuation"
            | "punctuation.bracket"
            | "punctuation.delimiter"
            | "punctuation.list_marker"
            | "punctuation.special"
            | "operator" => secondary(),
            _ => text(),
        };
        if !value.is_object() {
            *value = serde_json::json!({});
        }
        value["color"] = serde_json::to_value(color).expect("syntax color");
    }
    style.syntax = serde_json::from_value(syntax).expect("Canopy syntax palette");
}

/// Jira brand mark, shared with the Electron reference (Simple Icons CC0).
pub fn jira_brand() -> gpui_kit::Hsla {
    gpui_kit::rgb(0x2684ff).into()
}
