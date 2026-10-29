//! Settings → Appearance: theme family and Light/Dark/Auto mode, custom window
//! and sidebar backgrounds for each appearance, and the terminal font.
//!
//! Everything applies at once. Backgrounds are previewed live through
//! `theme::preview_background` and persisted through `theme::set_background`;
//! bursts of edits (color picker drags, key repeat) are coalesced into one write.
use super::search::{Entry, PageSearch, SettingsSearch};
use super::*;
use crate::app_state::SettingsState;
use crate::ui::components::{
    background::{SurfacePaint, paint_canvas},
    gradient_editor::{GradientEditor, GradientEditorEvent, readability_warning},
};
use crate::ui::theme::{
    AppearanceMode, Background, BackgroundMode, BackgroundTarget, FamilyInfo, HexColor, Preview,
    ThemeVariant,
};
use canopy_desktop::settings::{Change, TERMINAL_FONT_SIZES};
use gpui_kit::{
    component::{
        button::ButtonVariants,
        color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
        input::{InputEvent, NumberInput},
        scroll::ScrollableElement,
    },
    prelude::FluentBuilder,
};
use std::time::Duration;

/// Quiet time after the last edit of a burst before it is written.
const COMMIT_DELAY: Duration = Duration::from_millis(250);
const CARD_WIDTH: f32 = 118.;
const CARD_GAP: f32 = 10.;
/// Longest accepted terminal font family list, as in the settings contract.
const FONT_FAMILY_LIMIT: usize = 256;

fn color_of(color: HexColor) -> Hsla {
    rgb(color.0).into()
}
fn hex_of(color: Hsla) -> HexColor {
    let color: Rgba = color.into();
    let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    HexColor((channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b))
}
fn other(target: BackgroundTarget) -> BackgroundTarget {
    match target {
        BackgroundTarget::Window => BackgroundTarget::Sidebar,
        BackgroundTarget::Sidebar => BackgroundTarget::Window,
    }
}
fn help(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .line_height(px(14.85))
        .text_color(t::muted())
        .child(text.into())
}

const MODE: Entry = Entry::new(
    "mode",
    "THEME",
    "Mode",
    "Light, dark, or following the system",
    "appearance light dark auto system macos windows",
);
const BACKGROUND_APPEARANCE: Entry = Entry::new(
    "background-appearance",
    "BACKGROUND",
    "Dark appearance and light appearance",
    "Each appearance has its own backgrounds",
    "variant",
);
const BACKGROUND_SURFACE: Entry = Entry::new(
    "background-surface",
    "BACKGROUND",
    "Window and sidebar",
    "The surface whose background is edited; use the same background for both",
    "surface copy",
);
const BACKGROUND_STYLE: Entry = Entry::new(
    "background-style",
    "BACKGROUND",
    "Theme, color or gradient",
    "Your own color or gradient behind the window and the sidebar",
    "solid color hex picker gradient angle stops preset wallpaper readability",
);
const FONT_FAMILY: Entry = Entry::new(
    "font-family",
    "TERMINAL FONT",
    "Font family",
    "Comma-separated list of fonts; the first available font is used",
    "typeface monospace jetbrains mono",
);
const FONT_SIZE: Entry = Entry::new(
    "font-size",
    "TERMINAL FONT",
    "Font size",
    "Terminal text size in pixels (8–24)",
    "text px",
);
/// Search entries, in page order: the mode, a card per theme family, then the rest.
pub(super) fn entries() -> Vec<Entry> {
    let mut entries = vec![MODE];
    entries.extend(t::families().into_iter().map(|info| {
        Entry::new(
            info.id,
            "THEME",
            info.name,
            "Color theme",
            "colors palette scheme",
        )
    }));
    entries.extend([
        BACKGROUND_APPEARANCE,
        BACKGROUND_SURFACE,
        BACKGROUND_STYLE,
        FONT_FAMILY,
        FONT_SIZE,
    ]);
    entries
}

/// An edit waiting for the end of its burst.
struct Pending {
    surface: (ThemeVariant, BackgroundTarget),
    background: Background,
    _timer: Task<()>,
}

pub(super) struct AppearancePreferences {
    settings: Entity<SettingsState>,
    search: Entity<SettingsSearch>,
    scroll: ScrollHandle,
    target: BackgroundTarget,
    /// The appearance whose backgrounds are edited; `None` follows the one shown.
    editing: Option<ThemeVariant>,
    /// The surface the editor and fields were last loaded from.
    loaded: Option<(ThemeVariant, BackgroundTarget)>,
    editor: Entity<GradientEditor>,
    solid: Entity<ColorPickerState>,
    solid_hex: Entity<InputState>,
    font_family: Entity<InputState>,
    font_size: Entity<InputState>,
    pending: Option<Pending>,
    _subscriptions: Vec<Subscription>,
}

impl AppearancePreferences {
    pub(super) fn new(
        search: Entity<SettingsSearch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = cx.global::<AppState>().settings.clone();
        let variant = t::variant();
        let target = BackgroundTarget::Window;
        let background = t::background_for(variant, target, cx);
        let terminal = settings.read(cx).values.terminal.clone();
        let editor = cx.new(|cx| GradientEditor::new(&background.gradient, variant, window, cx));
        let solid = cx
            .new(|cx| ColorPickerState::new(window, cx).default_value(color_of(background.solid)));
        let solid_hex = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(background.solid.to_hex())
                .validate(|text, _| {
                    let hex = text.strip_prefix('#').unwrap_or(text);
                    hex.len() <= 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
                })
        });
        let font_family = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(terminal.font_family.clone())
                .placeholder(canopy_desktop::settings::DEFAULT_TERMINAL_FONT_FAMILY)
        });
        let font_size = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(terminal.font_size.to_string())
                .min(*TERMINAL_FONT_SIZES.start() as f64)
                .max(*TERMINAL_FONT_SIZES.end() as f64)
                .step(1.)
                .validate(|text, _| text.len() <= 2 && text.chars().all(|c| c.is_ascii_digit()))
        });
        let subscriptions = vec![
            cx.observe(&search, |_, _, cx| cx.notify()),
            cx.observe_in(&settings, window, |this, _, window, cx| {
                this.load(window, cx)
            }),
            cx.subscribe_in(&editor, window, |this, _, event, _, cx| match event {
                GradientEditorEvent::Preview(gradient) => {
                    let mut background = this.edited(cx);
                    background.mode = BackgroundMode::Gradient;
                    background.gradient = gradient.clone();
                    t::preview_background(this.variant(), this.target, Some(background), cx);
                }
                GradientEditorEvent::Change(gradient) => {
                    let gradient = gradient.clone();
                    this.edit(
                        move |background| {
                            background.mode = BackgroundMode::Gradient;
                            background.gradient = gradient;
                        },
                        true,
                        cx,
                    );
                }
            }),
            cx.subscribe_in(&solid, window, |this, _, event, window, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                let color = hex_of(*color);
                if this.edited(cx).solid != color {
                    this.edit(move |background| background.solid = color, true, cx);
                    this.sync_solid(window, cx);
                }
            }),
            cx.subscribe_in(&solid_hex, window, |this, input, event, window, cx| {
                if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    return;
                }
                let text = input.read(cx).value();
                let text = text.trim().trim_start_matches('#');
                match HexColor::parse(&format!("#{text}")) {
                    Some(color) if color != this.edited(cx).solid => {
                        this.edit(move |background| background.solid = color, false, cx);
                    }
                    _ => {}
                }
                this.sync_solid(window, cx);
            }),
            cx.subscribe_in(&font_family, window, |this, input, event, window, cx| {
                if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    return;
                }
                let value = input.read(cx).value().trim().to_owned();
                let current = this.settings.read(cx).values.terminal.font_family.clone();
                let valid = !value.is_empty()
                    && value.len() <= FONT_FAMILY_LIMIT
                    && !value.chars().any(char::is_control);
                if valid && value != current {
                    this.settings.update(cx, |settings, cx| {
                        settings.remember(vec![Change::TerminalFontFamily(value)], cx)
                    });
                } else {
                    input.update(cx, |input, cx| input.set_value(current, window, cx));
                }
            }),
            cx.subscribe_in(&font_size, window, |this, input, event, window, cx| {
                let current = this.settings.read(cx).values.terminal.font_size;
                match event {
                    InputEvent::Change => {
                        if let Ok(size) = input.read(cx).value().trim().parse::<u8>()
                            && TERMINAL_FONT_SIZES.contains(&size)
                            && size != current
                        {
                            this.settings.update(cx, |settings, cx| {
                                settings.remember(vec![Change::TerminalFontSize(size)], cx)
                            });
                        }
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        let valid = input
                            .read(cx)
                            .value()
                            .trim()
                            .parse::<u8>()
                            .is_ok_and(|size| TERMINAL_FONT_SIZES.contains(&size));
                        if !valid {
                            input.update(cx, |input, cx| {
                                input.set_value(current.to_string(), window, cx)
                            });
                        }
                    }
                    _ => {}
                }
            }),
        ];
        // An edit still waiting for its burst to end is written when the page closes.
        cx.on_release(|this, cx| this.flush(cx)).detach();
        Self {
            settings,
            search,
            scroll: ScrollHandle::new(),
            target,
            editing: None,
            loaded: Some((variant, target)),
            editor,
            solid,
            solid_hex,
            font_family,
            font_size,
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    fn variant(&self) -> ThemeVariant {
        self.editing.unwrap_or_else(t::variant)
    }
    /// The background being edited, as shown (a live preview included).
    fn edited(&self, cx: &App) -> Background {
        t::background_for(self.variant(), self.target, cx)
    }

    /// Applies `change` to the edited background: shown at once, written now or
    /// after a short quiet time (`coalesce`). The preview stays until the write
    /// is committed, so the surface never flickers back.
    fn edit(
        &mut self,
        change: impl FnOnce(&mut Background),
        coalesce: bool,
        cx: &mut Context<Self>,
    ) {
        let surface = (self.variant(), self.target);
        let mut background = self.edited(cx);
        change(&mut background);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.surface != surface)
        {
            self.flush(cx);
        }
        t::preview_background(surface.0, surface.1, Some(background.clone()), cx);
        if coalesce {
            let timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(COMMIT_DELAY).await;
                let _ = this.update(cx, |this, cx| this.flush(cx));
            });
            self.pending = Some(Pending {
                surface,
                background,
                _timer: timer,
            });
        } else {
            self.pending = None;
            t::set_background(surface.0, surface.1, background, cx);
        }
        cx.notify();
    }

    /// Writes the edit waiting for its burst to end, if any.
    fn flush(&mut self, cx: &mut App) {
        if let Some(pending) = self.pending.take() {
            let (variant, target) = pending.surface;
            t::set_background(variant, target, pending.background, cx);
        }
    }

    /// Loads the edited surface into the gradient editor and the fields; fields
    /// being typed in keep their text.
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (variant, target) = (self.variant(), self.target);
        self.loaded = Some((variant, target));
        let background = self.edited(cx);
        self.editor.update(cx, |editor, cx| {
            editor.set_gradient(&background.gradient, variant, window, cx)
        });
        self.sync_solid(window, cx);
        let terminal = self.settings.read(cx).values.terminal.clone();
        for (field, text) in [
            (&self.font_family, terminal.font_family),
            (&self.font_size, terminal.font_size.to_string()),
        ] {
            let focused = field.read(cx).focus_handle(cx).is_focused(window);
            if !focused && field.read(cx).value().as_ref() != text {
                field.update(cx, |field, cx| field.set_value(text, window, cx));
            }
        }
        cx.notify();
    }

    fn sync_solid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let solid = self.edited(cx).solid;
        if self.solid.read(cx).value().map(hex_of) != Some(solid) {
            self.solid.update(cx, |picker, cx| {
                picker.set_value(color_of(solid), window, cx)
            });
        }
        let focused = self.solid_hex.read(cx).focus_handle(cx).is_focused(window);
        if !focused && self.solid_hex.read(cx).value().as_ref() != solid.to_hex() {
            self.solid_hex
                .update(cx, |input, cx| input.set_value(solid.to_hex(), window, cx));
        }
    }

    fn search(&self, cx: &App) -> PageSearch {
        self.search.read(cx).page("Appearance").scroll(&self.scroll)
    }

    fn theme_section(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let search = self.search(cx);
        let current = t::current(cx);
        let content = f32::from(window.viewport_size().width) - SIDEBAR - 2. * CONTENT_X;
        let columns = ((content + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .clamp(2., 6.) as u16;
        let system = if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "the system"
        };
        let shown = match current.variant {
            ThemeVariant::Dark => "dark",
            ThemeVariant::Light => "light",
        };
        let mode = row()
            .flex_wrap()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(13.))
                    .child(search.text(MODE.id, MODE.label)),
            )
            .child(segmented_control(
                "appearance-mode",
                "Appearance mode",
                &[
                    (AppearanceMode::Light, "Light"),
                    (AppearanceMode::Dark, "Dark"),
                    (AppearanceMode::Auto, "Auto"),
                ],
                current.mode,
                |mode, _, cx| t::set_mode(mode, cx),
            ))
            .children(
                (current.mode == AppearanceMode::Auto)
                    .then(|| help(format!("Follows {system}: the app is {shown} now."))),
            );
        let cards = div().grid().grid_cols(columns).gap(px(CARD_GAP)).children(
            t::families().into_iter().enumerate().map(|(index, info)| {
                search.mark(
                    &[info.id],
                    theme_card(index, &info, current.variant, info.family == current.family),
                )
            }),
        );
        section(
            "THEME",
            "Colors of the whole app: surfaces, text, accents, Git and terminal. Applies immediately to every window.",
            vec![
                column()
                    .gap(px(12.))
                    .child(search.mark(&[MODE.id], mode))
                    .child(cards)
                    .into_any_element(),
            ],
        )
    }

    fn background_section(&self, cx: &mut Context<Self>) -> Div {
        let search = self.search(cx);
        let variant = self.variant();
        let shown = t::variant();
        let background = self.edited(cx);
        let page = cx.entity().downgrade();
        let appearance = row()
            .flex_wrap()
            .gap(px(12.))
            .child(segmented_control(
                "background-appearance",
                "Appearance to edit",
                &[
                    (ThemeVariant::Dark, "Dark appearance"),
                    (ThemeVariant::Light, "Light appearance"),
                ],
                variant,
                {
                    let page = page.clone();
                    move |variant, window, cx| {
                        let _ = page.update(cx, |this, cx| {
                            this.editing = (variant != t::variant()).then_some(variant);
                            this.load(window, cx);
                        });
                    }
                },
            ))
            .children((variant != shown).then(|| {
                help(match variant {
                    ThemeVariant::Dark => "Shown while the app is dark.",
                    ThemeVariant::Light => "Shown while the app is light.",
                })
            }));
        let target_name = match self.target {
            BackgroundTarget::Window => "sidebar",
            BackgroundTarget::Sidebar => "window",
        };
        let controls = row()
            .flex_wrap()
            .gap(px(12.))
            .child(segmented_control(
                "background-target",
                "Surface",
                &[
                    (BackgroundTarget::Window, "Window"),
                    (BackgroundTarget::Sidebar, "Sidebar"),
                ],
                self.target,
                {
                    let page = page.clone();
                    move |target, window, cx| {
                        let _ = page.update(cx, |this, cx| {
                            this.target = target;
                            this.load(window, cx);
                        });
                    }
                },
            ))
            .child(segmented_control(
                "background-mode",
                "Background",
                &[
                    (BackgroundMode::Theme, "Theme"),
                    (BackgroundMode::Solid, "Color"),
                    (BackgroundMode::Gradient, "Gradient"),
                ],
                background.mode,
                {
                    let page = page.clone();
                    move |mode, _, cx| {
                        let _ = page.update(cx, |this, cx| {
                            this.edit(move |background| background.mode = mode, false, cx)
                        });
                    }
                },
            ))
            .children((background.mode != BackgroundMode::Theme).then(|| {
                button("background-copy", format!("Use for {target_name} too"))
                    .ghost()
                    .border_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.flush(cx);
                        let (variant, target) = (this.variant(), other(this.target));
                        let background = this.edited(cx);
                        t::preview_background(variant, target, Some(background.clone()), cx);
                        t::set_background(variant, target, background, cx);
                        cx.notify();
                    }))
            }));
        let editor = match background.mode {
            BackgroundMode::Theme => None,
            BackgroundMode::Solid => Some(
                column()
                    .gap(px(8.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .min_w(px(64.))
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child("Color"),
                            )
                            .child(
                                ColorPicker::new(&self.solid)
                                    .accessibility_label("Background color")
                                    .small(),
                            )
                            .child(
                                input(&self.solid_hex)
                                    .w(px(90.))
                                    .font_family(t::MONO)
                                    .text_size(px(12.)),
                            ),
                    )
                    .children(readability_warning([background.solid], variant).map(warning))
                    .into_any_element(),
            ),
            BackgroundMode::Gradient => Some(self.editor.clone().into_any_element()),
        };
        section(
            "BACKGROUND",
            "Your own color or gradient behind the window and the sidebar, for the dark and the light appearance. Panels, menus and cards keep the theme's solid colors so text stays readable.",
            vec![
                column()
                    .gap(px(12.))
                    .child(search.mark(&[BACKGROUND_APPEARANCE.id], appearance))
                    .child(search.mark(
                        &[
                            BACKGROUND_APPEARANCE.id,
                            BACKGROUND_SURFACE.id,
                            BACKGROUND_STYLE.id,
                        ],
                        self.preview(variant, cx),
                    ))
                    .child(search.mark(&[BACKGROUND_SURFACE.id, BACKGROUND_STYLE.id], controls))
                    .children(
                        editor
                            .map(|editor| search.mark(&[BACKGROUND_STYLE.id], div().child(editor))),
                    )
                    .into_any_element(),
            ],
        )
    }

    /// A miniature window — sidebar and work area — painted like the real one,
    /// with the edited surface outlined.
    fn preview(&self, variant: ThemeVariant, cx: &App) -> Div {
        let colors = t::preview(t::current(cx).family, variant);
        let paint =
            |target, theme| SurfacePaint::of(&t::background_for(variant, target, cx), theme);
        let sidebar = paint(BackgroundTarget::Sidebar, colors.sidebar);
        let window = paint(BackgroundTarget::Window, colors.bg);
        let radius = px(7.);
        let line = |width: f32, strong: bool| {
            div()
                .h(px(6.))
                .w(relative(width))
                .rounded(px(3.))
                .bg(if strong {
                    colors.text.opacity(0.55)
                } else {
                    colors.secondary.opacity(0.35)
                })
        };
        let item = |mark: Option<Hsla>, width: f32| {
            row()
                .gap(px(6.))
                .when(mark.is_none(), |row| row.pl(px(10.)))
                .children(mark.map(|mark| {
                    div()
                        .size(px(12.))
                        .flex_shrink_0()
                        .rounded(px(3.))
                        .bg(mark.opacity(0.35))
                }))
                .child(line(width, mark.is_some()))
        };
        let outline = div()
            .absolute()
            .top(px(4.))
            .bottom(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_dashed()
            .border_color(t::accent().opacity(0.7));
        let outline = match self.target {
            BackgroundTarget::Sidebar => outline.left(px(4.)).right(relative(0.66)).mr(px(4.)),
            BackgroundTarget::Window => outline.left(relative(0.34)).ml(px(4.)).right(px(4.)),
        };
        row()
            .relative()
            .h(px(150.))
            .items_stretch()
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .child(
                column()
                    .relative()
                    .w(relative(0.34))
                    .gap(px(6.))
                    .px(px(8.))
                    .py(px(10.))
                    .border_r_1()
                    .border_color(colors.border)
                    .child(
                        paint_canvas(
                            sidebar,
                            Corners {
                                top_left: radius,
                                bottom_left: radius,
                                ..Default::default()
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(item(Some(colors.accent), 0.6))
                    .child(item(None, 0.7))
                    .child(item(None, 0.5))
                    .child(item(Some(colors.yellow), 0.45))
                    .child(item(None, 0.62)),
            )
            .child(
                column()
                    .relative()
                    .flex_1()
                    .gap(px(7.))
                    .px(px(14.))
                    .py(px(12.))
                    .child(
                        paint_canvas(
                            window,
                            Corners {
                                top_right: radius,
                                bottom_right: radius,
                                ..Default::default()
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(line(0.72, true))
                    .child(line(0.88, false))
                    .child(line(0.64, false))
                    .child(
                        div()
                            .h(px(26.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.panel),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .h(px(34.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(colors.control_border)
                            .bg(colors.input_bg),
                    ),
            )
            .child(outline)
    }

    fn font_section(&self, cx: &App) -> Div {
        let search = self.search(cx);
        section(
            "TERMINAL FONT",
            "Font used in terminal panes. Changes apply immediately.",
            vec![
                search
                    .stacked_setting(
                        &FONT_FAMILY,
                        FONT_FAMILY.help,
                        true,
                        input(&self.font_family)
                            .w_full()
                            .font_family(t::MONO)
                            .text_size(px(12.)),
                    )
                    .into_any_element(),
                search
                    .setting(
                        &FONT_SIZE,
                        false,
                        NumberInput::new(&self.font_size).small().w(px(104.)),
                    )
                    .into_any_element(),
            ],
        )
    }
}

fn warning(text: &'static str) -> Div {
    row()
        .items_start()
        .gap(px(8.))
        .text_size(px(11.))
        .line_height(px(15.))
        .text_color(t::yellow())
        .child(
            icon(IconName::TriangleAlert)
                .size(px(13.))
                .text_color(t::yellow()),
        )
        .child(text)
}

/// A theme family card: a miniature of the variant shown now, its name and
/// accents. Selecting it switches every window at once.
fn theme_card(
    index: usize,
    info: &FamilyInfo,
    variant: ThemeVariant,
    selected: bool,
) -> Stateful<Div> {
    let colors: Preview = match variant {
        ThemeVariant::Dark => info.dark,
        ThemeVariant::Light => info.light,
    };
    let family = info.family;
    let bar = |color: Hsla, width: f32, opacity: f32| {
        div()
            .h(px(4.))
            .w(relative(width))
            .rounded(px(2.))
            .bg(color.opacity(opacity))
    };
    let dot = |color: Hsla| div().size(px(7.)).rounded_full().bg(color);
    column()
        .id(("theme-card", index))
        .p(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(if selected { t::accent() } else { t::border() })
        .when(selected, |card| {
            card.shadow(vec![BoxShadow {
                inset: false,
                color: t::accent(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(1.),
            }])
        })
        .when(!selected, |card| {
            card.hover(|style| style.border_color(t::control_border()))
        })
        .role(Role::RadioButton)
        .aria_label(info.name)
        .aria_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        })
        .tab_index(0)
        .focus_visible(|style| style.border_color(t::accent()))
        .on_click(move |_, _, cx| t::set_theme(family, cx))
        .child(
            row()
                .h(px(58.))
                .items_stretch()
                .rounded(px(5.))
                .bg(colors.bg)
                .child(
                    column()
                        .w(relative(0.3))
                        .gap(px(4.))
                        .px(px(4.))
                        .py(px(6.))
                        .rounded_l(px(5.))
                        .bg(colors.sidebar)
                        .child(bar(colors.text, 0.8, 0.55))
                        .child(bar(colors.text, 0.6, 0.55))
                        .child(bar(colors.accent, 0.7, 0.9)),
                )
                .child(
                    column()
                        .flex_1()
                        .gap(px(4.))
                        .p(px(6.))
                        .child(bar(colors.text, 0.7, 0.8))
                        .child(bar(colors.text, 0.9, 0.4))
                        .child(bar(colors.text, 0.55, 0.4))
                        .child(div().flex_1())
                        .child(bar(colors.accent, 0.3, 1.)),
                ),
        )
        .child(
            row()
                .mt(px(6.))
                .px(px(2.))
                .gap(px(6.))
                .text_size(px(12.))
                .when(selected, |row| {
                    row.child(icon(IconName::Check).size(px(12.)).text_color(t::accent()))
                })
                .child(div().min_w_0().truncate().child(info.name))
                .child(
                    row()
                        .ml_auto()
                        .flex_shrink_0()
                        .gap(px(3.))
                        .child(dot(colors.accent))
                        .child(dot(colors.green))
                        .child(dot(colors.red)),
                ),
        )
}

impl Render for AppearancePreferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The shown appearance can change under the page (Auto follows the OS).
        if self.loaded != Some((self.variant(), self.target)) {
            cx.defer_in(window, |this, window, cx| this.load(window, cx));
        }
        let state = self.settings.read(cx);
        let error = state.error.clone();
        let loading = !state.ready && error.is_none();
        column()
            .size_full()
            .child(
                column()
                    .px(px(CONTENT_X))
                    .pt(px(20.))
                    .pb(px(12.))
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(px(16.1))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Appearance"),
                    )
                    .child(help(
                        "Color theme, window and sidebar background, and terminal font",
                    )),
            )
            .child(
                column()
                    .id("prefs-appearance")
                    .flex_1()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(
                        column()
                            .flex_shrink_0()
                            .gap(px(SECTION_GAP))
                            .children(error.map(|error| div().text_color(t::red()).child(error)))
                            .children(loading.then(|| div().child("Loading settings…")))
                            .child(self.theme_section(window, cx))
                            .child(self.background_section(cx))
                            .child(self.font_section(cx)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's `test` attribute; `gpui_kit::test` expands to this one.
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext, component::Root};

    #[gpui_kit::test]
    fn edits_show_at_once_and_stay_until_the_settings_commit_them(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            t::init(cx);
            let workspace = canopy_desktop::state::workspace::Workspace::empty(
                canopy_desktop::state::projects::Projects::default()
                    .open(std::env::current_dir().unwrap()),
            );
            AppState::install_test(workspace, cx);
            cx.open_window(Default::default(), |window, cx| {
                t::bind(window, cx);
                let search = SettingsSearch::detached("Appearance", cx);
                let page = cx.new(|cx| AppearancePreferences::new(search, window, cx));
                cx.new(|cx| Root::new(page, window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let page = cx.update(|window, cx| {
            window
                .root::<Root>()
                .flatten()
                .unwrap()
                .read(cx)
                .view()
                .clone()
                .downcast::<AppearancePreferences>()
                .unwrap()
        });
        let shown = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| t::background(BackgroundTarget::Window, cx))
        };
        assert_eq!(shown(&mut cx).mode, BackgroundMode::Theme);

        // A settled edit is shown before the settings owner has written it.
        page.update(&mut cx, |page, cx| {
            page.edit(|b| b.mode = BackgroundMode::Gradient, false, cx)
        });
        assert_eq!(shown(&mut cx).mode, BackgroundMode::Gradient);
        // Another settings change (still the old background) does not undo it.
        let settings = cx.update(|_, cx| cx.global::<AppState>().settings.clone());
        settings.update(&mut cx, |settings, cx| {
            settings.values.notch_enabled = false;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(shown(&mut cx).mode, BackgroundMode::Gradient);

        // Once committed, the persisted value takes over from the preview.
        let committed = shown(&mut cx);
        settings.update(&mut cx, |settings, cx| {
            *settings
                .values
                .appearance
                .backgrounds
                .get_mut(t::variant(), BackgroundTarget::Window) = committed.clone();
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(shown(&mut cx), committed);
        settings.update(&mut cx, |settings, cx| {
            settings
                .values
                .appearance
                .backgrounds
                .get_mut(t::variant(), BackgroundTarget::Window)
                .mode = BackgroundMode::Solid;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            shown(&mut cx).mode,
            BackgroundMode::Solid,
            "no stale preview"
        );

        // A burst is shown at once and written once it has settled.
        page.update(&mut cx, |page, cx| {
            page.edit(|b| b.solid = HexColor(0x123456), true, cx)
        });
        assert!(page.read_with(&cx, |page, _| page.pending.is_some()));
        assert_eq!(shown(&mut cx).solid, HexColor(0x123456));
    }
}
