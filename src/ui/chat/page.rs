//! Pages agents show in the chat (DEC-076..078). Each `show_html` call is a card that
//! draws a snapshot of its page. A card on screen also gets its live page (WKWebView)
//! right away, warming up transparent and out of the pointer's way; while the pointer is
//! on the card and nothing of Canopy's covers it, that page is shown, already loaded and
//! settled. Menus, dialogs and the find bar always stay on top; scrolling stays smooth.
use super::{rows::Row, view::ChatView};
use crate::ui::{components::*, html_native, theme as t};
use canopy_desktop::chat::{
    page::{self, Page},
    transcript::{ToolCall, ToolState},
};
use gpui_kit::component::IconName;
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

/// Height of a page before it has reported its own.
const PLACEHOLDER: f32 = 240.;
/// Live pages kept at most; the least recently used goes first.
const LIVE: usize = 6;
/// A snapshot is taken again when the card's width changes more than this.
const RESHOOT: f32 = 4.;

/// Where a card was painted, and in which frame.
type Placed = Rc<Cell<(u64, Bounds<Pixels>)>>;

struct Live {
    page: Rc<html_native::Page>,
    /// The page has drawn: the snapshot under it can go.
    ready: bool,
    _events: Task<()>,
}

pub(super) struct PageCard {
    page: Page,
    /// The HTML with local images inlined, once prepared.
    html: Option<Arc<str>>,
    height: f32,
    snapshot: Option<(Arc<Image>, f32)>,
    error: Option<String>,
    live: Option<Live>,
    hovered: bool,
    used: u64,
    placed: Placed,
    /// The row it was last drawn in (to remeasure it).
    row: Cell<u64>,
    shooting: bool,
    /// Width of the last render; a failed one isn't retried at the same width.
    tried: Option<f32>,
    _tasks: Vec<Task<()>>,
}

/// The chat's pages, by tool call.
#[derive(Default)]
pub(super) struct Pages {
    cards: HashMap<String, PageCard>,
    /// Cards drawn before they were prepared; set up at the next render.
    wanted: RefCell<Vec<(String, Page)>>,
    frame: Rc<Cell<u64>>,
    tick: u64,
}

/// Canopy's theme for pages: CSS variables on :root and a transparent page.
pub(crate) fn theme_css() -> String {
    let css = |color: Hsla| {
        let rgba: Rgba = color.into();
        format!(
            "rgba({:.0},{:.0},{:.0},{:.3})",
            rgba.r * 255.,
            rgba.g * 255.,
            rgba.b * 255.,
            rgba.a
        )
    };
    let font = crate::ui::platform_ui::interface_font();
    format!(
        ":root{{--background:{};--foreground:{};--muted:{};--muted-foreground:{};--card:{};\
         --card-foreground:{};--border:{};--accent:{};--red:{};--green:{};--yellow:{};--radius:8px;\
         --font-sans:\"{font}\",-apple-system,system-ui,sans-serif;\
         --font-mono:\"{}\",ui-monospace,Menlo,monospace;color-scheme:dark}}\
         html,body{{margin:0;padding:0;background:transparent;color:var(--foreground);\
         font-family:var(--font-sans);font-size:14px;line-height:1.5}}",
        css(t::bg()),
        css(t::text()),
        css(t::hover()),
        css(t::secondary()),
        css(t::elevated()),
        css(t::text()),
        css(t::control_border()),
        css(t::accent()),
        css(t::red()),
        css(t::green()),
        css(t::yellow()),
        t::MONO,
    )
}

impl ChatView {
    /// Sets up pages drawn since the last render and keeps their snapshots current.
    pub(super) fn sync_pages(&mut self, scale: f32, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = std::mem::take(&mut *self.pages.wanted.borrow_mut());
        for (call, page) in wanted {
            if self.pages.cards.contains_key(&call) {
                continue;
            }
            let source = page.html.clone();
            let id = call.clone();
            let remote = self.rt(cx).remote_source().and_then(|s| {
                let key = s
                    .projection
                    .items
                    .iter()
                    .find_map(|item| match &item.body {
                        Some(canopy_desktop::remote::proto::transcript_item::Body::Tool(tool))
                            if item.key.is_some_and(|key| {
                                canopy_desktop::remote::client::presentation::call_id(
                                    key.history_epoch,
                                    &tool.call_id,
                                ) == call
                            }) =>
                        {
                            tool.page_item
                        }
                        _ => None,
                    })?;
                Some((s.host.clone()?, s.chat(), key))
            });
            let download = remote.and_then(|(host, chat, key)| {
                crate::ui::remote_client::executor(cx).ok().map(|runtime| {
                    runtime.spawn(async move {
                        let bytes = host.read_chat_page(chat, key).await?;
                        let html = String::from_utf8(bytes)
                            .map_err(|_| "Invalid UTF-8 page.".to_owned())?;
                        Ok::<_, String>(crate::ui::remote_client::isolated_html(&html))
                    })
                })
            });
            let prepare = cx.spawn(async move |this, cx| {
                let result = if let Some(download) = download {
                    download.await.map_err(|e| e.to_string()).and_then(|r| r)
                } else {
                    Ok(cx
                        .background_executor()
                        .spawn(async move { page::inline_images(&source) })
                        .await)
                };
                let _ = this.update(cx, |this, cx| {
                    if let Some(card) = this.pages.cards.get_mut(&id) {
                        match result {
                            Ok(html) => card.html = Some(html.into()),
                            Err(error) => card.error = Some(error),
                        }
                        cx.notify();
                    }
                });
            });
            self.pages.cards.insert(
                call,
                PageCard {
                    height: page.height.unwrap_or(PLACEHOLDER),
                    page,
                    html: None,
                    snapshot: None,
                    error: None,
                    live: None,
                    hovered: false,
                    used: 0,
                    placed: Rc::new(Cell::new((0, Bounds::default()))),
                    row: Cell::new(0),
                    shooting: false,
                    tried: None,
                    _tasks: vec![prepare],
                },
            );
        }
        let calls: Vec<String> = self
            .pages
            .cards
            .iter()
            .filter(|(_, card)| {
                let width = f32::from(card.placed.get().1.size.width);
                card.html.is_some()
                    && !card.shooting
                    && width > 0.
                    && card.tried.is_none_or(|at| (at - width).abs() > RESHOOT)
            })
            .map(|(call, _)| call.clone())
            .collect();
        for call in calls {
            self.shoot(call, scale, cx);
        }
        self.warm_pages(window, cx);
    }

    /// Cards painted in the last frame get their live page now, so it has loaded and
    /// played its animations before the pointer comes.
    fn warm_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last = self.pages.frame.get().saturating_sub(1);
        let cold: Vec<String> = self
            .pages
            .cards
            .iter()
            .filter(|(_, card)| {
                let painted = card.placed.get().0;
                card.live.is_none()
                    && card.html.is_some()
                    && card.error.is_none()
                    && painted > 0
                    && painted >= last
            })
            .map(|(call, _)| call.clone())
            .collect();
        // Room for them first: pages scrolled away give way to pages on screen.
        self.retire_pages();
        let on_screen = self
            .pages
            .cards
            .values()
            .filter(|card| card.live.is_some() && card.placed.get().0 >= last)
            .count();
        for call in cold.into_iter().take(LIVE.saturating_sub(on_screen)) {
            self.pages.tick += 1;
            let tick = self.pages.tick;
            let Some(card) = self.pages.cards.get_mut(&call) else {
                continue;
            };
            let Some(html) = card.html.clone() else {
                continue;
            };
            card.used = tick;
            match html_native::Page::new(&html, &theme_css(), page::MAX_HEIGHT, window) {
                Ok((native, events)) => {
                    let id = call.clone();
                    let pump = cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            let alive = this
                                .update(cx, |this, cx| this.page_event(&id, event, cx))
                                .is_ok();
                            if !alive {
                                break;
                            }
                        }
                    });
                    card.live = Some(Live {
                        page: Rc::new(native),
                        ready: false,
                        _events: pump,
                    });
                }
                Err(error) => card.error = Some(error),
            }
        }
        self.retire_pages();
    }

    /// Renders the page off screen at the card's width for its snapshot and height.
    fn shoot(&mut self, call: String, scale: f32, cx: &mut Context<Self>) {
        let Some(card) = self.pages.cards.get_mut(&call) else {
            return;
        };
        let Some(html) = card.html.clone() else {
            return;
        };
        let width = f32::from(card.placed.get().1.size.width);
        card.shooting = true;
        card.tried = Some(width);
        let id = call.clone();
        let task = cx.spawn(async move |this, cx| {
            let started = html_native::capture(&html, &theme_css(), width, page::MAX_HEIGHT, scale);
            let result = match started {
                Ok((capture, _console, images)) => {
                    let timeout = cx.background_executor().timer(Duration::from_secs(15));
                    let image =
                        futures_lite::future::or(async { images.recv().await.ok() }, async {
                            timeout.await;
                            None
                        })
                        .await;
                    drop(capture);
                    image.ok_or_else(|| "The page took too long to render.".to_owned())
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                let Some(card) = this.pages.cards.get_mut(&id) else {
                    return;
                };
                card.shooting = false;
                match result {
                    Ok(captured) => {
                        if !captured.png.is_empty() {
                            let image = Arc::new(Image::from_bytes(ImageFormat::Png, captured.png));
                            card.snapshot = Some((image, width));
                        }
                        if captured.height > 0. {
                            card.height = captured.height.clamp(40., page::MAX_HEIGHT);
                        }
                        card.error = None;
                    }
                    // The last snapshot stays.
                    Err(error) => card.error = Some(error),
                }
                let row = card.row.get();
                this.remeasure_row(row, cx);
                cx.notify();
            });
        });
        if let Some(card) = self.pages.cards.get_mut(&call) {
            card._tasks.push(task);
        }
    }

    fn remeasure_row(&mut self, key: u64, cx: &mut Context<Self>) {
        let rows = self.shown_rows();
        if let Some(index) = rows.rows.iter().position(|row| row.key == key) {
            rows.scroller
                .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx));
        }
    }

    /// The pointer entered or left a card: its warmed-up page is shown or put away.
    fn hover_page(&mut self, call: &str, hovered: bool, cx: &mut Context<Self>) {
        self.pages.tick += 1;
        let tick = self.pages.tick;
        if let Some(card) = self.pages.cards.get_mut(call) {
            card.hovered = hovered;
            card.used = tick;
        }
        cx.notify();
    }

    fn page_event(&mut self, call: &str, event: html_native::PageEvent, cx: &mut Context<Self>) {
        let Some(card) = self.pages.cards.get_mut(call) else {
            return;
        };
        if let html_native::PageEvent::Height(height) = event {
            if let Some(live) = &mut card.live {
                live.ready = true;
            }
            let height = height.clamp(40., page::MAX_HEIGHT);
            if (height - card.height).abs() > 1. {
                card.height = height;
                let row = card.row.get();
                self.remeasure_row(row, cx);
            }
            cx.notify();
        }
    }

    /// Keeps at most `LIVE` live pages: those whose card is off screen go first, least
    /// recently used first. Pages on screen stay, so none is warmed again and again.
    fn retire_pages(&mut self) {
        let last = self.pages.frame.get().saturating_sub(1);
        let count = self
            .pages
            .cards
            .values()
            .filter(|card| card.live.is_some())
            .count();
        let mut away: Vec<(u64, String)> = self
            .pages
            .cards
            .iter()
            .filter(|(_, card)| card.live.is_some() && !card.hovered && card.placed.get().0 < last)
            .map(|(call, card)| (card.used, call.clone()))
            .collect();
        away.sort();
        for (_, call) in away.into_iter().take(count.saturating_sub(LIVE)) {
            if let Some(card) = self.pages.cards.get_mut(&call) {
                card.live = None;
            }
        }
    }

    /// Something of Canopy's is over the conversation: live pages give way.
    fn pages_covered(&self, window: &mut Window, cx: &mut App) -> bool {
        use gpui_kit::component::WindowExt as _;
        self.find.is_some()
            || self.quote_offer.is_some()
            || self.quote_reply.is_some()
            || self.suggest.is_some()
            || self.model_picker.open
            || self.attachments.lightbox.is_some()
            || self.drill.is_animating(std::time::Instant::now())
            || window.has_active_dialog(cx)
            || window.has_active_prompt()
    }

    /// Placed after the conversation's rows: lays every live page over where its card was
    /// painted this frame, cut to the conversation (shown when hovered and uncovered,
    /// warming up otherwise), and hides those whose card wasn't painted.
    pub(super) fn page_layer(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<impl IntoElement + use<>> {
        // Also before any page is live: this layer counts the frames by which cards
        // count as on screen, so their pages warm up at all.
        if self.pages.cards.is_empty() {
            return None;
        }
        let covered = self.pages_covered(window, cx);
        let pages: Vec<(Rc<html_native::Page>, Placed, bool)> = self
            .pages
            .cards
            .values()
            .filter_map(|card| {
                let live = card.live.as_ref()?;
                Some((
                    live.page.clone(),
                    card.placed.clone(),
                    card.hovered && !covered,
                ))
            })
            .collect();
        let frame = self.pages.frame.clone();
        Some(
            canvas(
                |_, _, _| (),
                move |clip, _, _, _| {
                    let current = frame.get();
                    for (page, placed, shown) in &pages {
                        let (painted, bounds) = placed.get();
                        if painted == current {
                            page.place(clip, bounds);
                            page.set_active(*shown);
                        } else {
                            page.hide();
                        }
                    }
                    frame.set(current + 1);
                },
            )
            .absolute()
            .inset_0(),
        )
    }

    /// The pages stop: the chat closes or the agent restarts its process.
    pub(super) fn drop_live_pages(&mut self) {
        for card in self.pages.cards.values_mut() {
            card.live = None;
        }
    }
}

/// Opens a page in the default browser from a private file in Canopy's data folder.
fn open_in_browser(call: &str, html: &str) -> Result<(), String> {
    use std::io::Write as _;
    let dir = canopy_desktop::platform::directories::data_dir()?.join("pages");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name: String = call
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(80)
        .collect();
    let path = dir.join(format!(
        "{}.html",
        if name.is_empty() { "page" } else { &name }
    ));
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    file.write_all(html.as_bytes()).map_err(|e| e.to_string())?;
    let url = gpui_kit::http_client::Url::from_file_path(&path)
        .map_err(|_| "Invalid page path.".to_owned())?;
    std::process::Command::new("/usr/bin/open")
        .arg(url.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The card of a `show_html` call.
pub(super) fn page_card(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    tool: &ToolCall,
) -> Div {
    let key = entry.key;
    let call = tool.call_id.clone();
    let parsed = Page::from_input(&tool.input);
    let card = this.pages.cards.get(&call);
    if card.is_none()
        && tool.state == ToolState::Succeeded
        && let Ok(page) = &parsed
    {
        this.pages
            .wanted
            .borrow_mut()
            .push((call.clone(), page.clone()));
    }
    let title = match (&parsed, card) {
        (_, Some(card)) => card.page.title.clone(),
        (Ok(page), None) => page.title.clone(),
        (Err(_), None) => "Page".to_owned(),
    };
    let header = row()
        .h(px(24.))
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(t::muted())
        .child(custom_icon("code").size(px(12.)))
        .child(div().flex_1().min_w_0().truncate().child(title))
        .children(card.and_then(|card| card.html.clone()).map(|html| {
            let open = (call.clone(), html.clone());
            let copy = html;
            row()
                .gap(px(2.))
                .invisible()
                .group_hover("chat-page", |style| style.visible())
                .child(
                    icon_button(
                        ("chat-page-open", key),
                        IconName::ExternalLink,
                        "Open in browser",
                    )
                    .on_click(move |_, _, _| {
                        if let Err(error) = open_in_browser(&open.0, &open.1) {
                            eprintln!("Page open: {error}");
                        }
                    }),
                )
                .child(
                    icon_button(("chat-page-copy", key), IconName::Copy, "Copy HTML").on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()))
                        },
                    ),
                )
        }));
    let body: AnyElement = match (card, &parsed) {
        (Some(card), _) => {
            card.row.set(key);
            let placed = card.placed.clone();
            let frame = this.pages.frame.clone();
            // The next render needs this card's size: for its first snapshot, and to warm
            // its live page (while there is room for one).
            let live_pages = this
                .pages
                .cards
                .values()
                .filter(|c| c.live.is_some())
                .count();
            let settling = card.snapshot.is_none() && card.tried.is_none()
                || card.live.is_none()
                    && card.html.is_some()
                    && card.error.is_none()
                    && live_pages < LIVE;
            let showing_live = card.hovered && card.live.as_ref().is_some_and(|live| live.ready);
            let view = view.downgrade();
            let hover_call = call.clone();
            div()
                .id(("chat-page", key))
                .relative()
                .w_full()
                .h(px(card.height))
                .overflow_hidden()
                .on_hover(move |hovered: &bool, _, cx| {
                    let _ = view.update(cx, |this, cx| this.hover_page(&hover_call, *hovered, cx));
                })
                .when(!showing_live, |area| {
                    area.children(card.snapshot.as_ref().map(|(image, width)| {
                        img(image.clone())
                            .w(px(*width))
                            .h(px(card.height))
                            .object_fit(ObjectFit::Fill)
                    }))
                    .when(card.snapshot.is_none(), |area| {
                        area.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.))
                                .text_color(t::faint())
                                .child(card.error.clone().unwrap_or_else(|| "Rendering…".into())),
                        )
                    })
                })
                // Where the live page goes: recorded while this frame paints.
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            placed.set((frame.get(), bounds));
                            if settling {
                                window.request_animation_frame();
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element()
        }
        // Set up at the next render, which this asks for.
        (None, Ok(_)) => div()
            .relative()
            .h(px(PLACEHOLDER))
            .child(
                canvas(
                    |_, _, _| (),
                    |_, _, window, _| window.request_animation_frame(),
                )
                .absolute()
                .inset_0(),
            )
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(t::faint())
            .child(if tool.state.finished() {
                "Rendering…"
            } else {
                "Preparing the page…"
            })
            .into_any_element(),
        (None, Err(error)) => div()
            .text_size(px(12.))
            .text_color(t::red())
            .child(error.clone())
            .into_any_element(),
    };
    column()
        .pt(px(12.))
        .group("chat-page")
        .gap(px(4.))
        .child(header)
        .child(body)
}

/// Tallest image a preview for an agent shows; the reported height is the full one.
const PREVIEW_HEIGHT: f32 = 4000.;

/// Answers agents' `preview_html` requests: each page renders off screen on the main
/// thread (local images inlined first), within the service's deadline.
pub(crate) fn serve_previews<T: 'static>(
    requests: async_channel::Receiver<canopy_desktop::chat::pages::Request>,
    cx: &mut Context<T>,
) -> Task<()> {
    cx.spawn(async move |_, cx| {
        while let Ok(request) = requests.recv().await {
            let source = request.html.clone();
            let html = cx
                .background_executor()
                .spawn(async move { page::inline_images(&source) })
                .await;
            let width = request.width;
            // Agents get the page at 1x: a smaller image says as much.
            let started =
                cx.update(|_| html_native::capture(&html, &theme_css(), width, PREVIEW_HEIGHT, 1.));
            let result = match started {
                Ok((capture, console, images)) => {
                    let timeout = cx
                        .background_executor()
                        .timer(canopy_desktop::chat::pages::DEADLINE - Duration::from_secs(5));
                    let image =
                        futures_lite::future::or(async { images.recv().await.ok() }, async {
                            timeout.await;
                            None
                        })
                        .await;
                    let mut lines = vec![];
                    while let Ok(event) = console.try_recv() {
                        if let html_native::PageEvent::Console(line) = event {
                            lines.push(line);
                        }
                    }
                    lines.truncate(200);
                    drop(capture);
                    match image {
                        Some(image) if !image.png.is_empty() => {
                            Ok(canopy_desktop::chat::pages::Preview {
                                png: image.png,
                                height: image.height,
                                console: lines,
                            })
                        }
                        Some(_) => Err("The page rendered nothing to capture.".to_owned()),
                        None => Err("The page took too long to render.".to_owned()),
                    }
                }
                Err(error) => Err(error),
            };
            request.reply(result);
        }
    })
}
