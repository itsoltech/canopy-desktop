//! Canopy's design tokens over a runtime theme.
//!
//! Every color token reads the active theme variant: a family (`ThemeFamily`) in
//! its dark or light variant, chosen by `appearance.theme` and `appearance.mode`
//! (Light, Dark or Auto, which follows the OS appearance). Base colors come from
//! `canopy_desktop::appearance`; secondary tokens are derived here per variant
//! and precomputed, so a token read during render is one atomic load and a copy.
//! `bind` keeps the active variant, the gpui-component `Theme`, the native window
//! appearance and terminal palettes in step with the settings owner.
use crate::app_state::AppState;
pub use canopy_desktop::appearance::{
    AppearanceMode, Background, BackgroundMode, BackgroundTarget, GRADIENT_MAX_STOPS,
    GRADIENT_MIN_STOPS, Gradient, GradientStop, HexColor, ThemeFamily, ThemeVariant,
};
use canopy_desktop::{
    appearance::{self, Backgrounds, VariantSpec},
    settings::{Change, TerminalPreferences},
    terminal::geometry::CellSize,
};
use gpui_kit::component::{Theme, ThemeMode, highlighter::HighlightTheme};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

// Shared application palette. Semantic accents use OKLCH; neutral surfaces and
// text are opaque so their contrast is consistent across every layer.
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
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
/// OKLCH (lightness, chroma, hue in degrees) of an sRGB color.
fn to_oklch(color: Hsla) -> (f32, f32, f32) {
    let c: Rgba = color.into();
    let (r, g, b) = (linear(c.r), linear(c.g), linear(c.b));
    let l = (0.41222147 * r + 0.53633254 * g + 0.05144599 * b).cbrt();
    let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
    let s = (0.08830246 * r + 0.28171884 * g + 0.6299787 * b).cbrt();
    let lightness = 0.21045426 * l + 0.7936178 * m - 0.004072047 * s;
    let a = 1.9779985 * l - 2.4285922 * m + 0.4505937 * s;
    let bb = 0.025904037 * l + 0.78277177 * m - 0.80867577 * s;
    (lightness, a.hypot(bb), bb.atan2(a).to_degrees())
}
fn hex(color: u32) -> Hsla {
    rgb(color).into()
}
/// Opaque sRGB interpolation from `a` (t = 0) to `b` (t = 1).
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: 1.,
    }
    .into()
}
fn luminance(color: Hsla) -> f32 {
    let c: Rgba = color.into();
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}
/// WCAG contrast ratio of two opaque colors.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
/// `color` with its OKLCH lightness moved away from `bg` until the contrast
/// reaches `min`; hue and chroma are kept.
fn legible(color: Hsla, bg: Hsla, min: f32) -> Hsla {
    if contrast(color, bg) >= min {
        return color;
    }
    let (l, c, h) = to_oklch(color);
    let step = if luminance(bg) > 0.18 { -0.01 } else { 0.01 };
    (1..=60)
        .map(|n| oklch((l + step * n as f32).clamp(0., 1.), c, h, 1.))
        .find(|candidate| contrast(*candidate, bg) >= min)
        .unwrap_or(color)
}
/// Text faded towards `bg` by `ratio`, but never below `min` contrast.
fn faded(text: Hsla, bg: Hsla, ratio: f32, min: f32) -> Hsla {
    let mut ratio = ratio;
    while ratio > 0. {
        let candidate = mix(text, bg, ratio);
        if contrast(candidate, bg) >= min {
            return candidate;
        }
        ratio -= 0.02;
    }
    text
}

/// Every color token of one variant; precomputed so render reads are cheap.
#[derive(Clone, Copy)]
struct Palette {
    light: bool,
    bg: Hsla,
    sidebar: Hsla,
    scroll_edge_shadow: Hsla,
    elevated: Hsla,
    panel: Hsla,
    text: Hsla,
    secondary: Hsla,
    muted: Hsla,
    faint: Hsla,
    border: Hsla,
    control_border: Hsla,
    input_bg: Hsla,
    hover: Hsla,
    selected: Hsla,
    accent: Hsla,
    green: Hsla,
    red: Hsla,
    yellow: Hsla,
    badge_ink: Hsla,
}

/// The colors Canopy had before themes, kept exactly.
fn canopy_dark() -> Palette {
    Palette {
        light: false,
        bg: hex(0x191c22),
        sidebar: hex(0x15181d),
        scroll_edge_shadow: hex(0x101318),
        elevated: hex(0x232832),
        panel: hex(0x1c2027),
        text: hex(0xe9edf3),
        secondary: hex(0xa9b1bd),
        muted: hex(0x9ca6b4),
        faint: hex(0x808b9a),
        border: hex(0x2b323d),
        control_border: hex(0x46515f),
        input_bg: hex(0x171b21),
        hover: hex(0x1e252e),
        selected: hex(0x25364b),
        accent: hex(0x83b7ff),
        green: hex(0x59d3b2),
        red: oklch(0.712, 0.181, 22.84, 1.),
        yellow: hex(0xf2bf70),
        badge_ink: hex(0x10131a),
    }
}

/// Secondary tokens from a variant's base colors. Dark variants raise surfaces
/// towards the text color; light variants sink them, with hover and selection
/// based on the darker of window and sidebar so both show them.
fn derive(spec: &VariantSpec, light: bool) -> Palette {
    let (bg, sidebar, text) = (hex(spec.bg), hex(spec.sidebar), hex(spec.text));
    let (black, white) = (hex(0x000000), hex(0xffffff));
    let semantic = |color: u32| legible(hex(color), bg, 3.);
    let accent = semantic(spec.accent);
    let (darker, lighter) = if luminance(sidebar) < luminance(bg) {
        (sidebar, bg)
    } else {
        (bg, sidebar)
    };
    let shared = |palette: Palette| Palette {
        light,
        bg,
        sidebar,
        text,
        accent,
        green: semantic(spec.green),
        red: semantic(spec.red),
        yellow: semantic(spec.yellow),
        ..palette
    };
    if light {
        shared(Palette {
            scroll_edge_shadow: mix(sidebar, text, 0.3),
            elevated: mix(bg, text, 0.045),
            panel: mix(bg, white, 0.6),
            secondary: faded(text, bg, 0.2, 4.5),
            muted: faded(text, bg, 0.3, 3.8),
            faint: faded(text, bg, 0.42, 3.),
            border: mix(bg, text, 0.12),
            control_border: mix(bg, text, 0.26),
            input_bg: mix(bg, white, 0.6),
            hover: mix(darker, text, 0.055),
            selected: mix(darker, accent, 0.16),
            badge_ink: mix(bg, white, 0.7),
            ..canopy_dark()
        })
    } else {
        shared(Palette {
            scroll_edge_shadow: mix(darker, black, 0.3),
            elevated: mix(bg, text, 0.06),
            panel: mix(bg, text, 0.025),
            secondary: faded(text, bg, 0.3, 4.5),
            muted: faded(text, bg, 0.36, 4.),
            faint: faded(text, bg, 0.48, 3.),
            border: mix(bg, text, 0.09),
            control_border: mix(bg, text, 0.2),
            input_bg: mix(bg, black, 0.1),
            hover: mix(lighter, text, 0.035),
            selected: mix(lighter, accent, 0.17),
            badge_ink: mix(bg, black, 0.4),
            ..canopy_dark()
        })
    }
}

fn palette_of(family: ThemeFamily, variant: ThemeVariant) -> Palette {
    if (family, variant) == (ThemeFamily::Canopy, ThemeVariant::Dark) {
        canopy_dark()
    } else {
        derive(family.spec(variant), variant.is_light())
    }
}

static PALETTES: LazyLock<[Palette; appearance::VARIANT_COUNT]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        palette_of(ThemeFamily::ALL[index / 2], ThemeVariant::ALL[index % 2])
    })
});
fn p() -> &'static Palette {
    &PALETTES[appearance::active_index()]
}

/// The active variant is light: pick light-mode treatments (overlays, marks).
pub fn is_light() -> bool {
    p().light
}
pub fn variant() -> ThemeVariant {
    appearance::active().1
}

pub fn bg() -> Hsla {
    p().bg
}
/// Machine-readable QR modules require fixed contrast independent of app theme.
pub fn qr_light() -> Hsla {
    rgb(0xffffff).into()
}
pub fn qr_dark() -> Hsla {
    rgb(0x000000).into()
}
pub fn sidebar() -> Hsla {
    p().sidebar
}
/// Neutral edge shadow, distinct from the sidebar without using pure black.
pub fn scroll_edge_shadow() -> Hsla {
    p().scroll_edge_shadow
}
pub fn elevated() -> Hsla {
    p().elevated
}
pub fn text() -> Hsla {
    p().text
}
pub fn secondary() -> Hsla {
    p().secondary
}
pub fn muted() -> Hsla {
    p().muted
}
pub fn faint() -> Hsla {
    p().faint
}
pub fn border() -> Hsla {
    p().border
}
pub fn control_border() -> Hsla {
    p().control_border
}
pub fn input_bg() -> Hsla {
    p().input_bg
}
/// The message field's surface, opaque: the conversation scrolls beneath the composer.
pub fn composer_surface() -> Hsla {
    bg().blend(input_bg())
}
pub fn modal_overlay() -> Hsla {
    oklch(0., 0., 0., if is_light() { 0.25 } else { 0.4 })
}
pub fn hover() -> Hsla {
    p().hover
}
pub fn selected() -> Hsla {
    p().selected
}
pub fn accent() -> Hsla {
    p().accent
}
pub fn green() -> Hsla {
    p().green
}
pub fn red() -> Hsla {
    p().red
}
pub fn yellow() -> Hsla {
    p().yellow
}
/// Violet of the chat's skill badges (`$name`).
pub fn skill() -> Hsla {
    if is_light() {
        oklch(0.52, 0.16, 305., 1.)
    } else {
        oklch(0.76, 0.13, 305., 1.)
    }
}
/// Teal of the chat's goal (`/goal`): its composer mode, row and badge.
pub fn goal() -> Hsla {
    if is_light() {
        oklch(0.52, 0.1, 185., 1.)
    } else {
        oklch(0.80, 0.13, 185., 1.)
    }
}
/// Neutral light on the reasoning slider: the sheen in its gradient, its knob and
/// filled stops. It sits on the colored track, so it stays light in every theme.
pub fn effort_sheen() -> Hsla {
    white()
}
/// The reasoning slider at `intensity` 0..1 (lowest to highest effort): a calm indigo
/// warming through violet to a hot pink.
pub fn effort(intensity: f32) -> Hsla {
    let i = intensity.clamp(0., 1.);
    let lightness = if is_light() { 0.5 } else { 0.6 };
    oklch(lightness + 0.08 * i, 0.16 + 0.05 * i, 268. + 92. * i, 1.)
}
/// Burgundy of the chat composer's shell mode (`!command`) and shell command cards.
pub fn shell() -> Hsla {
    oklch(if is_light() { 0.52 } else { 0.64 }, 0.17, 6., 1.)
}
/// The light travelling over shimmering status text: brighter than the text on
/// dark themes, fainter than it on light ones.
pub fn shimmer_highlight() -> Hsla {
    if is_light() {
        mix(text(), bg(), 0.6)
    } else {
        white()
    }
}

pub fn claude_mark() -> Hsla {
    if is_light() {
        oklch(0.6, 0.15, 42., 1.)
    } else {
        oklch(0.75, 0.14, 42., 1.)
    }
}
pub fn codex_mark() -> Hsla {
    if is_light() {
        oklch(0.52, 0.11, 162., 1.)
    } else {
        oklch(0.80, 0.13, 162., 1.)
    }
}
/// Brand colors of tool icons; OpenAI's mark is monochrome and follows the text.
pub fn claude_brand() -> Hsla {
    rgb(0xd97757).into()
}
pub fn openai_brand() -> Hsla {
    text()
}
pub fn lazygit_brand() -> Hsla {
    rgb(0xf05033).into()
}
pub fn chat_selected() -> Hsla {
    selected()
}
pub fn chat_hover() -> Hsla {
    hover()
}

/// Identity hues of project monogram tiles; a project keeps its hue by name.
const PROJECT_HUES: [f32; 8] = [214., 300., 32., 150., 185., 262., 350., 95.];
pub fn project_hue(name: &str) -> f32 {
    let hash = name
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    PROJECT_HUES[hash as usize % PROJECT_HUES.len()]
}
pub fn project_tile(hue: f32) -> Hsla {
    if is_light() {
        oklch(0.92, 0.04, hue, 1.)
    } else {
        oklch(0.32, 0.06, hue, 1.)
    }
}
pub fn project_tile_text(hue: f32) -> Hsla {
    if is_light() {
        oklch(0.42, 0.1, hue, 1.)
    } else {
        oklch(0.86, 0.09, hue, 1.)
    }
}
/// Provider tile behind an agent's mark: a wash and a hairline of the mark's color.
pub fn agent_tile(mark: Hsla) -> Hsla {
    mark.opacity(0.14)
}
pub fn agent_tile_edge(mark: Hsla) -> Hsla {
    mark.opacity(0.18)
}
/// Glyphs drawn on a bright status badge.
pub fn badge_ink() -> Hsla {
    p().badge_ink
}
/// One soft sweep over a row whose agent starts waiting for the user.
pub fn attention_flash() -> Hsla {
    yellow().opacity(0.22)
}

pub const SPACING_UNIT: f32 = 4.;
pub fn chat_row_height() -> Rems {
    rems(2.75)
}
/// Provider tile of a session row; the activity edge is traced around it.
pub fn agent_tile_size() -> Rems {
    rems(1.5)
}
pub fn agent_tile_radius() -> Rems {
    rems(0.4375)
}
pub fn project_tile_size() -> Rems {
    rems(1.25)
}
/// Corner radius of sidebar rows and their primary controls.
pub fn sidebar_row_radius() -> Pixels {
    px(8.)
}
pub fn navigation_height() -> Rems {
    rems(2.25)
}
pub fn chat_metadata_line_height() -> Rems {
    rems(1.)
}
/// Hover cards: wide enough for a branch, a short path or a pull request title.
pub fn hover_card_width() -> Rems {
    rems(16.)
}
pub fn chat_title_line_height() -> Rems {
    rems(1.25)
}
pub fn control_radius(cx: &App) -> Pixels {
    cx.global::<Theme>().radius
}
pub const TITLEBAR: f32 = 36.;
pub const ROW: f32 = 28.;
pub fn tabs_height() -> Rems {
    rems(2.25)
}
pub fn tab_min_width() -> Rems {
    rems(7.)
}
pub fn tab_max_width() -> Rems {
    rems(18.)
}
/// Widest host name in a tab's badge, in rems.
pub const TAB_BADGE_TEXT: f32 = 6.;
/// Gradient editor stop handles sit on arbitrary user colors: a white ring and
/// a dark shadow keep them visible on any of them, in every theme.
pub fn gradient_handle_ring() -> Hsla {
    white()
}
pub fn gradient_handle_shadow() -> Hsla {
    black().opacity(0.6)
}
/// Hairline around a user-colored gradient bar.
pub fn gradient_bar_edge() -> Hsla {
    text().opacity(0.12)
}
/// Lift under the selected segment of a segmented control.
pub fn segment_shadow() -> Hsla {
    black().opacity(if is_light() { 0.12 } else { 0.35 })
}
pub fn navigation_overlay() -> Hsla {
    black().opacity(if is_light() { 0.3 } else { 0.65 })
}
pub fn navigation_shadow() -> BoxShadow {
    BoxShadow {
        inset: false,
        color: black().opacity(if is_light() { 0.16 } else { 0.5 }),
        offset: point(px(8.), px(0.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
    }
}
pub fn panel() -> Hsla {
    p().panel
}
pub const STATUS: f32 = 24.;
pub const MONO: &str = "JetBrains Mono";

/// Electron's terminal fallback chain, preferring single-cell Nerd glyphs. It
/// follows the families the user lists in `terminal.font_family`.
const TERMINAL_FALLBACKS: [&str; 8] = [
    MONO,
    "JetBrainsMono Nerd Font Mono",
    "JetBrainsMono NFM",
    "JetBrainsMono Nerd Font",
    "JetBrainsMono NF",
    "Symbols Nerd Font Mono",
    "FiraCode Nerd Font",
    "Menlo",
];

/// The terminal's font as the painter uses it: family with fallbacks, size and
/// the cell measured from them.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalFont {
    pub font: Font,
    pub size: Pixels,
    pub cell: CellSize,
}
impl Default for TerminalFont {
    fn default() -> Self {
        let (family, fallbacks) = terminal_font_stack(MONO, None);
        Self {
            font: terminal_font_of(family, fallbacks),
            size: px(canopy_desktop::settings::DEFAULT_TERMINAL_FONT_SIZE as f32),
            cell: CellSize::DEFAULT,
        }
    }
}
struct TerminalFontState(TerminalFont);
impl Global for TerminalFontState {}

/// The terminal font chosen in Settings (the default until settings load).
pub fn terminal_font(cx: &App) -> TerminalFont {
    cx.try_global::<TerminalFontState>()
        .map(|state| state.0.clone())
        .unwrap_or_default()
}

/// Primary family and fallbacks for a comma-separated family list: the first
/// installed family of the list, then of Canopy's chain, is primary; the rest
/// stay as glyph fallbacks. Without the installed list the first name is used.
fn terminal_font_stack(
    list: &str,
    installed: Option<&std::collections::HashSet<String>>,
) -> (String, Vec<String>) {
    let mut candidates: Vec<String> = Vec::new();
    let names = list
        .split(',')
        .map(|name| name.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .chain(TERMINAL_FALLBACKS);
    for name in names {
        if !name.is_empty()
            && !candidates
                .iter()
                .any(|known| known.eq_ignore_ascii_case(name))
        {
            candidates.push(name.to_owned());
        }
    }
    let primary = installed
        .and_then(|installed| {
            candidates
                .iter()
                .position(|name| installed.iter().any(|font| font.eq_ignore_ascii_case(name)))
        })
        .unwrap_or(0);
    let family = candidates.remove(primary);
    (family, candidates)
}

fn terminal_font_of(family: String, fallbacks: Vec<String>) -> Font {
    Font {
        family: family.into(),
        features: FontFeatures(std::sync::Arc::new(vec![
            ("calt".into(), 0),
            ("liga".into(), 0),
            ("dlig".into(), 0),
        ])),
        fallbacks: Some(FontFallbacks::from_fonts(fallbacks)),
        ..Default::default()
    }
}

/// Resolves the terminal settings into a font and measures its cell. Called by
/// the theme owner when the settings or the installed font list change.
fn resolve_terminal_font(
    preferences: &canopy_desktop::settings::TerminalPreferences,
    installed: Option<&std::collections::HashSet<String>>,
    cx: &App,
) -> TerminalFont {
    let (family, fallbacks) = terminal_font_stack(&preferences.font_family, installed);
    let font = terminal_font_of(family, fallbacks);
    let size = preferences.font_size as f32;
    let text = cx.text_system();
    let advance = text
        .advance(text.resolve_font(&font), px(size), 'm')
        .ok()
        .map(|advance| f32::from(advance.width));
    TerminalFont {
        font,
        size: px(size),
        cell: CellSize::for_font(size, advance),
    }
}

/// Installs the gpui-component theme for the active variant. Startup and tests
/// call it once; `bind` re-applies it on every theme change.
pub fn init(cx: &mut App) {
    let light = is_light();
    Theme::change(
        if light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    let theme = Theme::global_mut(cx);
    theme.font_family = super::platform_ui::interface_font().into();
    // Electron rem is 16px; body typography is set explicitly by each view.
    theme.font_size = px(16.);
    theme.mono_font_family = MONO.into();
    theme.colors.background = bg();
    theme.colors.foreground = text();
    theme.colors.muted_foreground = muted();
    theme.colors.secondary = elevated();
    theme.colors.secondary_foreground = secondary();
    theme.colors.border = border();
    theme.colors.primary = accent();
    theme.colors.primary_foreground = bg();
    theme.colors.input = control_border();
    theme.colors.muted = elevated();
    theme.colors.popover = panel();
    theme.colors.popover_foreground = text();
    theme.colors.accent = hover();
    theme.colors.accent_foreground = text();
    theme.colors.ring = accent();
    theme.colors.button = hover();
    theme.colors.button_foreground = secondary();
    theme.colors.button_hover = selected();
    theme.colors.button_active = selected();
    theme.colors.button_primary = accent();
    theme.colors.button_primary_foreground = bg();
    theme.colors.button_primary_hover = accent();
    theme.colors.button_primary_active = accent();
    editor_theme(theme, light);
    // Built-in panels use ThemeTokens while inputs also read ThemeColor.
    // Keep both projections aligned after applying Canopy's overrides.
    theme.tokens = (&theme.colors).into();
    theme.radius = px(4.);
    canopy_desktop::motion::apply_to_theme(theme);
    Theme::sync_base(cx);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
/// The notch is a black hardware-like overlay in every theme; its colors are fixed.
pub fn notch_idle() -> Hsla {
    oklch(0.8, 0.182, 151.71, 1.)
}
/// The notch stays black in light themes too, so its text and status colors come
/// from the dark variant of the active family.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn notch_palette() -> &'static Palette {
    &PALETTES[appearance::index(appearance::active().0, ThemeVariant::Dark)]
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_muted() -> Hsla {
    notch_palette().muted
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_attention() -> Hsla {
    notch_palette().yellow
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_error() -> Hsla {
    notch_palette().red
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_working() -> Hsla {
    notch_palette().accent
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn notch_done() -> Hsla {
    notch_palette().green
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
    let (lightness, chroma) = if is_light() {
        (0.52, 0.12)
    } else {
        (0.792, 0.115)
    };
    match index % 6 {
        0 => accent(),
        1 => green(),
        2 => yellow(),
        3 => oklch(lightness - 0.01, chroma, 305., 1.),
        4 => red(),
        _ => oklch(lightness + 0.01, chroma, 185., 1.),
    }
}

/// Native editor chrome and syntax share Canopy's semantic palette. A fresh
/// highlight theme per change: code highlighters cache by its `Arc`.
fn editor_theme(theme: &mut Theme, light: bool) {
    theme.colors.caret = accent();
    theme.colors.selection = accent().opacity(0.22);
    let mut highlight = if light {
        HighlightTheme::default_light()
    } else {
        HighlightTheme::default_dark()
    }
    .as_ref()
    .clone();
    highlight.name = "Canopy".into();
    highlight.appearance = if light {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
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
    theme.highlight_theme = Arc::new(highlight);
}

/// Jira brand mark, shared with the Electron reference (Simple Icons CC0).
pub fn jira_brand() -> gpui_kit::Hsla {
    gpui_kit::rgb(0x2684ff).into()
}

// ---------------------------------------------------------------------------
// Theme selection: API for Settings → Appearance and the runtime owner.

/// Colors of one variant for a theme picker or background preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    /// Variant name, e.g. "Catppuccin Latte".
    pub name: &'static str,
    pub bg: Hsla,
    pub sidebar: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub accent: Hsla,
    pub green: Hsla,
    pub red: Hsla,
    pub yellow: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
    pub control_border: Hsla,
    pub input_bg: Hsla,
}
/// A selectable theme family with previews of both variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FamilyInfo {
    pub family: ThemeFamily,
    /// Stored identifier (`appearance.theme`).
    pub id: &'static str,
    pub name: &'static str,
    pub dark: Preview,
    pub light: Preview,
}
/// Colors of one variant of `family`.
pub fn preview(family: ThemeFamily, variant: ThemeVariant) -> Preview {
    let palette = &PALETTES[appearance::index(family, variant)];
    Preview {
        name: family.spec(variant).name,
        bg: palette.bg,
        sidebar: palette.sidebar,
        text: palette.text,
        secondary: palette.secondary,
        accent: palette.accent,
        green: palette.green,
        red: palette.red,
        yellow: palette.yellow,
        panel: palette.panel,
        border: palette.border,
        control_border: palette.control_border,
        input_bg: palette.input_bg,
    }
}

/// Every theme family in display order, with the colors the app will use.
pub fn families() -> Vec<FamilyInfo> {
    ThemeFamily::ALL
        .into_iter()
        .map(|family| FamilyInfo {
            family,
            id: family.key(),
            name: family.name(),
            dark: preview(family, ThemeVariant::Dark),
            light: preview(family, ThemeVariant::Light),
        })
        .collect()
}

/// What is shown: the chosen family and mode and the variant they resolve to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub family: ThemeFamily,
    pub mode: AppearanceMode,
    pub variant: ThemeVariant,
}
/// The applied theme (the persisted choice once `bind` has seen it).
pub fn current(cx: &App) -> Resolved {
    let (family, variant) = appearance::active();
    let mode = cx
        .try_global::<AppState>()
        .map(|app| app.settings.read(cx).values.appearance.mode)
        .unwrap_or_default();
    Resolved {
        family,
        mode,
        variant,
    }
}
/// Persists a choice through the settings owner; `bind` applies it to every
/// window once committed. Queued, never dropped, while settings load or save.
fn remember(change: Change, cx: &mut App) {
    let Some(settings) = cx.try_global::<AppState>().map(|app| app.settings.clone()) else {
        return;
    };
    settings.update(cx, |settings, cx| settings.remember(vec![change], cx));
}
pub fn set_theme(family: ThemeFamily, cx: &mut App) {
    remember(Change::AppearanceTheme(family), cx);
}
pub fn set_mode(mode: AppearanceMode, cx: &mut App) {
    remember(Change::AppearanceMode(mode), cx);
}
/// Persists a background. A live preview of the same surface stays shown until
/// the settings owner has committed it, so the surface never flickers back.
pub fn set_background(
    variant: ThemeVariant,
    target: BackgroundTarget,
    background: Background,
    cx: &mut App,
) {
    if let Some(runtime) = cx.try_global::<Runtime>()
        && runtime.previews.contains_key(&(variant, target))
    {
        cx.global_mut::<Runtime>()
            .previews
            .insert((variant, target), background.clone());
    }
    remember(Change::Background(variant, target, background), cx);
}
/// Shows `background` on `target` without persisting it (`None` drops the
/// preview), e.g. while a gradient stop is dragged. Every window repaints.
pub fn preview_background(
    variant: ThemeVariant,
    target: BackgroundTarget,
    background: Option<Background>,
    cx: &mut App,
) {
    if !cx.has_global::<Runtime>() {
        return;
    }
    let runtime = cx.global_mut::<Runtime>();
    let changed = match background {
        Some(background) => {
            runtime
                .previews
                .insert((variant, target), background.clone())
                != Some(background)
        }
        None => runtime.previews.remove(&(variant, target)).is_some(),
    };
    if changed {
        cx.refresh_windows();
    }
}
/// The background shown on `target` for `variant`: a live preview while one is
/// edited, otherwise the persisted value.
pub fn background_for(variant: ThemeVariant, target: BackgroundTarget, cx: &App) -> Background {
    if let Some(preview) = cx
        .try_global::<Runtime>()
        .and_then(|runtime| runtime.previews.get(&(variant, target)))
    {
        return preview.clone();
    }
    cx.try_global::<AppState>()
        .map(|app| {
            app.settings
                .read(cx)
                .values
                .appearance
                .backgrounds
                .get(variant, target)
                .clone()
        })
        .unwrap_or_else(|| Background::default_for(variant, target))
}
/// The background shown on `target` for the variant currently shown.
pub fn background(target: BackgroundTarget, cx: &App) -> Background {
    background_for(variant(), target, cx)
}

fn variant_of(appearance: WindowAppearance) -> ThemeVariant {
    match appearance {
        WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeVariant::Light,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeVariant::Dark,
    }
}

/// Owner of the applied theme: what was last applied and the native override,
/// live background previews and the terminal font.
struct Runtime {
    applied: Option<Resolved>,
    /// The app-wide native appearance last requested; `None` follows the OS.
    native: Option<Option<WindowAppearance>>,
    /// Backgrounds and terminal settings last painted; a change repaints windows.
    painted: Option<(Backgrounds, TerminalPreferences)>,
    /// Unsaved backgrounds shown while Settings edits them.
    previews: HashMap<(ThemeVariant, BackgroundTarget), Background>,
    /// Installed font families, read once off the UI thread.
    fonts: Option<HashSet<String>>,
    _fonts: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}
impl Global for Runtime {}

/// Follows the settings owner and the OS appearance of `window` (the main
/// window, which lives as long as the app). Call once after `AppState::init`.
pub fn bind(window: &Window, cx: &mut App) {
    let settings = cx.global::<AppState>().settings.clone();
    let subscriptions = vec![
        cx.observe(&settings, |_, cx| sync(None, cx)),
        window.observe_window_appearance(|window, cx| sync(Some(window.appearance()), cx)),
    ];
    // The installed families decide which listed terminal font is used.
    let text = cx.text_system().clone();
    let names = cx
        .background_executor()
        .spawn(async move { text.all_font_names() });
    let fonts = cx.spawn(async move |cx| {
        let names = names.await;
        cx.update(|cx| {
            let runtime = cx.global_mut::<Runtime>();
            runtime.fonts = Some(names.into_iter().collect());
            runtime.painted = None;
            sync(None, cx);
        });
    });
    cx.set_global(Runtime {
        applied: None,
        native: None,
        painted: None,
        previews: HashMap::new(),
        fonts: None,
        _fonts: Some(fonts),
        _subscriptions: subscriptions,
    });
    // Outside the window's own update: the native appearance change notifies it.
    cx.defer(|cx| sync(None, cx));
}

/// Resolves the persisted choice and applies it when it changed.
fn sync(system: Option<WindowAppearance>, cx: &mut App) {
    let Some(app) = cx.try_global::<AppState>() else {
        return;
    };
    let values = &app.settings.read(cx).values;
    let (choice, terminal) = (values.appearance.clone(), values.terminal.clone());
    let (family, mode) = (choice.theme, choice.mode);
    // Fixed modes also theme native chrome (menus, traffic lights); Auto clears
    // the override first, so the OS appearance read below is the system one.
    let native = match mode {
        AppearanceMode::Auto => None,
        AppearanceMode::Light => Some(WindowAppearance::Light),
        AppearanceMode::Dark => Some(WindowAppearance::Dark),
    };
    if cx.global::<Runtime>().native != Some(native) {
        cx.global_mut::<Runtime>().native = Some(native);
        cx.set_window_appearance(native);
    }
    let system = match mode {
        AppearanceMode::Auto => variant_of(system.unwrap_or_else(|| cx.window_appearance())),
        _ => ThemeVariant::Dark,
    };
    let resolved = Resolved {
        family,
        mode,
        variant: mode.resolve(system),
    };
    // Previews already committed give way to the persisted values.
    let runtime = cx.global_mut::<Runtime>();
    runtime
        .previews
        .retain(|(variant, target), preview| choice.backgrounds.get(*variant, *target) != preview);
    let refont = runtime
        .painted
        .as_ref()
        .is_none_or(|(_, painted)| *painted != terminal);
    let repaint = refont
        || runtime
            .painted
            .as_ref()
            .is_none_or(|(painted, _)| *painted != choice.backgrounds);
    runtime.painted = Some((choice.backgrounds.clone(), terminal.clone()));
    if refont {
        let font = resolve_terminal_font(&terminal, cx.global::<Runtime>().fonts.as_ref(), cx);
        // Terminal views re-measure their grid from it on the next frame.
        cx.set_global(TerminalFontState(font));
    }
    if cx.global::<Runtime>().applied == Some(resolved) {
        if repaint {
            cx.refresh_windows();
        }
        return;
    }
    cx.global_mut::<Runtime>().applied = Some(resolved);
    if appearance::active() != (family, resolved.variant) {
        appearance::set_active(family, resolved.variant);
        init(cx);
        // Terminal frames carry resolved colors: re-project them.
        let terminals = cx.global::<AppState>().terminals.read(cx).all();
        for (_, view) in terminals {
            view.update(cx, |view, _| view.repaint_palette());
        }
    }
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's `test` attribute; these are plain tests.
    use core::prelude::v1::test;

    #[test]
    fn every_variant_keeps_text_and_semantic_colors_legible() {
        for family in ThemeFamily::ALL {
            for variant in ThemeVariant::ALL {
                let palette = palette_of(family, variant);
                let name = family.spec(variant).name;
                assert_eq!(palette.light, variant.is_light(), "{name}");
                assert!(contrast(palette.text, palette.bg) >= 4.5, "{name} text");
                assert!(
                    contrast(palette.secondary, palette.bg) >= 4.5,
                    "{name} secondary"
                );
                assert!(contrast(palette.faint, palette.bg) >= 3., "{name} faint");
                for color in [palette.accent, palette.green, palette.red, palette.yellow] {
                    assert!(contrast(color, palette.bg) >= 3., "{name} semantic");
                }
                // Hover and selection are visible on both the window and the sidebar.
                for surface in [palette.bg, palette.sidebar] {
                    assert!(
                        contrast(palette.selected, surface) > 1.05,
                        "{name} selected"
                    );
                    assert!(contrast(palette.hover, surface) > 1.03, "{name} hover");
                }
            }
        }
    }

    #[test]
    fn the_first_installed_terminal_family_is_used_and_the_rest_fall_back() {
        let installed: HashSet<String> = ["Menlo", "Fira Code"].map(String::from).into();
        let (family, fallbacks) =
            terminal_font_stack("'Iosevka', fira code , \"Menlo\"", Some(&installed));
        assert_eq!(family, "fira code");
        // Unavailable families stay as glyph fallbacks, before Canopy's chain;
        // names are not repeated.
        assert_eq!(fallbacks[0], "Iosevka");
        assert_eq!(fallbacks[1], "Menlo");
        assert_eq!(
            fallbacks
                .iter()
                .filter(|f| f.eq_ignore_ascii_case("menlo"))
                .count(),
            1
        );
        assert!(fallbacks.iter().any(|f| f == "Symbols Nerd Font Mono"));
        // Nothing listed is installed: Canopy's chain picks Menlo.
        let (family, _) = terminal_font_stack("Nope", Some(&installed));
        assert_eq!(family, "Menlo");
        // Before the installed list is known the first listed family is used.
        assert_eq!(terminal_font_stack(" , Iosevka", None).0, "Iosevka");
    }

    #[test]
    fn canopy_dark_keeps_the_original_colors() {
        let palette = palette_of(ThemeFamily::Canopy, ThemeVariant::Dark);
        assert_eq!(palette.bg, hex(0x191c22));
        assert_eq!(palette.selected, hex(0x25364b));
        assert!(!palette.light);
        let light = palette_of(ThemeFamily::Canopy, ThemeVariant::Light);
        assert!(light.light && luminance(light.bg) > luminance(light.text));
    }

    #[test]
    fn oklch_round_trips() {
        for color in [0x83b7ff, 0xd20f39, 0x14710a, 0x777777] {
            let (l, c, h) = to_oklch(hex(color));
            let back: Rgba = oklch(l, c, h, 1.).into();
            let original: Rgba = hex(color).into();
            assert!((back.r - original.r).abs() < 0.01, "{color:x}");
            assert!((back.g - original.g).abs() < 0.01, "{color:x}");
            assert!((back.b - original.b).abs() < 0.01, "{color:x}");
        }
    }
}
