//! Attachments (pasted, dropped or picked images and files) as badges inside the
//! message text — `what do you see in [shot.png (120 KB)]?` — their previews under
//! sent messages, and a full-size image view.
use super::view::ChatView;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    attachments::{self as prepare, MAX_IMAGES},
    history,
    session::Draft,
    transcript::Attachment,
};
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::component::{
    IconName,
    input::{InlineToken, InlineTokenClickEvent, InlineTokenContext, InputContent},
    spinner::Spinner,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const SENT_THUMB: f32 = 72.;
const TOKEN_PREFIX: &str = "attachment-";

pub(super) enum AttachState {
    Preparing,
    Ready(Attachment),
    Failed(String),
}

/// An attachment mentioned in the composer's text by its badge.
pub(super) struct Pending {
    pub id: u64,
    /// The badge's text in the message, e.g. `[shot.png (120 KB)]`.
    pub text: SharedString,
    pub state: AttachState,
    pub preview: Option<Arc<Image>>,
    pub image: bool,
}

/// Attachments of the composer and previews of sent images.
#[derive(Default)]
pub(super) struct Attachments {
    pub pending: Vec<Pending>,
    next: u64,
    /// Badges waiting for a window to be put into the text at the cursor.
    insert: Vec<InlineToken>,
    /// Thumbnails of images, by file name in the chat's folder.
    pub previews: HashMap<String, Arc<Image>>,
    loading: HashSet<String>,
    /// The image shown full size, and its presence.
    pub lightbox: Option<Arc<Image>>,
    pub lightbox_presence: Option<Presence>,
}

/// The folder of the chat's attached images.
fn folder(chat: Option<&str>) -> Result<PathBuf, String> {
    let chat = chat.ok_or("This chat can't keep attachments.")?;
    Ok(history::attachments_dir(
        &canopy_desktop::platform::directories::data_dir()?,
        chat,
    ))
}

fn format_of(media_type: &str) -> ImageFormat {
    match media_type {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/webp" => ImageFormat::Webp,
        "image/gif" => ImageFormat::Gif,
        _ => ImageFormat::Png,
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// "840 B", "120 KB", "3.4 MB".
pub(super) fn size_label(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}

fn token_id(id: u64) -> String {
    format!("{TOKEN_PREFIX}{id}")
}
fn id_of(token: &InlineToken) -> Option<u64> {
    token.id().strip_prefix(TOKEN_PREFIX)?.parse().ok()
}

/// Brackets and control characters would break the badge's text; names keep the rest.
fn clean(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() || c == ']' { ' ' } else { c })
        .collect()
}

impl ChatView {
    /// The ids of attachments whose badges are in the composer's text, in order.
    fn mentioned(&self, cx: &App) -> Vec<u64> {
        self.composer
            .read(cx)
            .tokens()
            .iter()
            .filter_map(|span| id_of(span.token()))
            .collect()
    }

    fn pending(&self, id: u64) -> Option<&Pending> {
        self.attachments.pending.iter().find(|p| p.id == id)
    }

    /// The composer's text with the attachments its badges mention.
    pub(super) fn composer_draft(&self, cx: &App) -> Draft {
        Draft {
            text: self.composer.read(cx).value().to_string(),
            attachments: self
                .mentioned(cx)
                .into_iter()
                .filter_map(|id| {
                    let pending = self.pending(id)?;
                    match &pending.state {
                        AttachState::Ready(attachment) => {
                            Some(attachment.clone().with_label(pending.text.to_string()))
                        }
                        _ => None,
                    }
                })
                .collect(),
        }
    }

    /// A mentioned attachment still preparing or failed: nothing is sent until then.
    pub(super) fn attachments_blocked(&self, cx: &App) -> bool {
        self.mentioned(cx).into_iter().any(|id| {
            self.pending(id)
                .is_none_or(|p| !matches!(p.state, AttachState::Ready(_)))
        })
    }

    /// Text and badges for a message put back into the composer; its attachments
    /// become pending again. A mention missing from the text is added at its end.
    fn draft_content(&mut self, draft: Draft, cx: &mut Context<Self>) -> InputContent {
        self.attachments.pending.clear();
        let mut text = draft.text;
        let mut spans = vec![];
        let mut from = 0;
        for attachment in draft.attachments {
            let id = self.next_attachment();
            let mention =
                attachment
                    .label()
                    .map(str::to_owned)
                    .unwrap_or_else(|| match &attachment {
                        Attachment::Image { file, .. } => format!("[{file}]"),
                        Attachment::Path { path, .. } => format!("[{}]", path.display()),
                    });
            let start = match text[from..].find(&mention) {
                Some(at) => from + at,
                None => {
                    if !text.is_empty() && !text.ends_with(char::is_whitespace) {
                        text.push(' ');
                    }
                    text.push_str(&mention);
                    text.len() - mention.len()
                }
            };
            from = start + mention.len();
            spans.push((
                start..from,
                id,
                mention.clone(),
                badge_name(&attachment, &mention),
            ));
            let preview = self.preview_of(&attachment, cx);
            self.attachments.pending.push(Pending {
                id,
                text: mention.into(),
                image: attachment.is_image(),
                state: AttachState::Ready(attachment),
                preview,
            });
        }
        let mut content = InputContent::new(text.clone());
        for (range, id, mention, name) in spans {
            match content.clone().with_token(
                range,
                InlineToken::new(token_id(id), mention).with_label(name),
            ) {
                Ok(with) => content = with,
                // Plain text then; the attachment still goes with the message.
                Err(_) => return InputContent::new(text),
            }
        }
        content
    }

    /// Puts a message back into the composer, attachments included.
    pub(super) fn set_composer_draft(
        &mut self,
        draft: Draft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = self.draft_content(draft, cx);
        self.composer.update(cx, |state, cx| {
            state.set_value(content, window, cx);
            state.focus(window, cx);
        });
    }

    /// `set_composer_draft` without a window at hand (applied at the next render).
    pub(super) fn restore_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        let content = self.draft_content(draft, cx);
        self.decision.composer = Some(super::decision::ComposerChange {
            text: Some(content),
            placeholder: super::decision::COMPOSER_PLACEHOLDER,
        });
    }

    /// Empties the composer after sending.
    pub(super) fn clear_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |state, cx| state.clean(window, cx));
        self.attachments.pending.clear();
        self.attachments.insert.clear();
    }

    fn next_attachment(&mut self) -> u64 {
        self.attachments.next += 1;
        self.attachments.next
    }

    /// A new attachment and its badge, put into the text at the cursor.
    fn add_pending(&mut self, name: &str, mention: String, state: AttachState, image: bool) -> u64 {
        let id = self.next_attachment();
        self.attachments
            .insert
            .push(InlineToken::new(token_id(id), mention.clone()).with_label(name.to_owned()));
        self.attachments.pending.push(Pending {
            id,
            text: mention.into(),
            state,
            preview: None,
            image,
        });
        id
    }

    /// Puts waiting badges into the text (from render, where a window is at hand).
    pub(super) fn insert_badges(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.attachments.insert.is_empty() {
            return;
        }
        let tokens = std::mem::take(&mut self.attachments.insert);
        let composer = self.composer.clone();
        cx.defer_in(window, move |_, window, cx| {
            composer.update(cx, |state, cx| {
                for token in tokens {
                    let before = state.value();
                    let cursor = state.cursor();
                    // A space keeps the badge apart from the word before it.
                    if before[..cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_whitespace())
                    {
                        state.insert(" ", window, cx);
                    }
                    if state.replace_with_token(token, window, cx).is_ok() {
                        state.insert(" ", window, cx);
                    }
                }
                state.focus(window, cx);
            });
        });
    }

    /// Images become image attachments, anything else a path for the agent to read.
    pub(super) fn attach_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            let image = prepare::is_image_path(&path);
            cx.spawn(async move |this, cx| {
                let read = cx
                    .background_spawn({
                        let path = path.clone();
                        async move {
                            let size = std::fs::metadata(&path).map(|m| m.len()).ok();
                            let bytes = image.then(|| prepare::read_image(&path));
                            (size, bytes)
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let name = clean(&name_of(&path));
                    match read {
                        (size, Some(bytes)) => {
                            let label = match size {
                                Some(size) => format!("{name} ({})", size_label(size)),
                                None => name.clone(),
                            };
                            this.attach_image(&label, bytes, cx);
                        }
                        (_, None) => {
                            let mention = format!("[{}]", clean(&path.display().to_string()));
                            let attachment = Attachment::Path {
                                path: path.clone(),
                                label: None,
                            };
                            this.add_pending(&name, mention, AttachState::Ready(attachment), false);
                            cx.notify();
                        }
                    }
                });
            })
            .detach();
        }
    }

    /// `label`: the badge's name and size; `bytes`: the image as read.
    fn attach_image(
        &mut self,
        label: &str,
        bytes: Result<Vec<u8>, String>,
        cx: &mut Context<Self>,
    ) {
        let images = self
            .attachments
            .pending
            .iter()
            .filter(|p| p.image && !matches!(p.state, AttachState::Failed(_)))
            .count();
        let mention = format!("[{label}]");
        let bytes = match bytes {
            Ok(_) if images >= MAX_IMAGES => Err(format!("Up to {MAX_IMAGES} images per message.")),
            other => other,
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                self.add_pending(label, mention, AttachState::Failed(error), true);
                cx.notify();
                return;
            }
        };
        let id = self.add_pending(label, mention, AttachState::Preparing, true);
        let chat = self.chat_id();
        cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_spawn(async move {
                    let dir = folder(chat.as_deref())?;
                    let attachment = prepare::prepare_image(&bytes, &dir)?;
                    let stored = prepare::image_bytes(&dir, &attachment);
                    Ok::<_, String>((attachment, stored))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = this.attachments.pending.iter().position(|p| p.id == id) else {
                    return;
                };
                match prepared {
                    Ok((attachment, stored)) => {
                        if let (
                            Attachment::Image {
                                file, media_type, ..
                            },
                            Some(bytes),
                        ) = (&attachment, stored)
                        {
                            let image = Arc::new(Image::from_bytes(format_of(media_type), bytes));
                            this.attachments
                                .previews
                                .insert(file.clone(), image.clone());
                            this.attachments.pending[index].preview = Some(image);
                        }
                        this.attachments.pending[index].state = AttachState::Ready(attachment);
                    }
                    Err(error) => {
                        this.attachments.pending[index].state = AttachState::Failed(error)
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// ⌘V with an image or copied files: they become badges instead of text.
    pub(super) fn paste(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let mut handled = false;
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    let label = format!(
                        "image-{}.{} ({})",
                        self.attachments.next + 1,
                        image.format.extension(),
                        size_label(image.bytes.len() as u64)
                    );
                    self.attach_image(&label, Ok(image.bytes.clone()), cx);
                    handled = true;
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    self.attach_paths(paths.paths().to_vec(), cx);
                    handled = true;
                }
                ClipboardEntry::String(_) => {}
            }
        }
        if handled {
            return true;
        }
        // A browser's "Copy Image" also puts the image's address as text (DEC-052).
        let single_url = item.text().is_some_and(|text| {
            let text = text.trim();
            (text.starts_with("http://") || text.starts_with("https://"))
                && !text.contains(char::is_whitespace)
        });
        if single_url && let Some(bytes) = canopy_desktop::platform::pasteboard::image() {
            let label = format!(
                "image-{} ({})",
                self.attachments.next + 1,
                size_label(bytes.len() as u64)
            );
            self.attach_image(&label, Ok(bytes), cx);
            return true;
        }
        false
    }

    /// The paperclip: pick images, files or folders.
    pub(super) fn pick_attachments(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |this, cx| this.attach_paths(paths, cx));
            }
        })
        .detach();
    }

    /// Runs `send` with `draft` once its saved images are loaded again (a message sent
    /// again after a restart); otherwise right away.
    pub(super) fn with_images(
        &mut self,
        draft: Draft,
        cx: &mut Context<Self>,
        send: impl FnOnce(&mut Self, Draft, &mut Context<Self>) + 'static,
    ) {
        if !prepare::needs_data(&draft.attachments) {
            send(self, draft, cx);
            return;
        }
        let chat = self.chat_id();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    let dir = folder(chat.as_deref())?;
                    let attachments = prepare::with_data(&dir, draft.attachments)?;
                    Ok::<_, String>(Draft {
                        text: draft.text,
                        attachments,
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match loaded {
                Ok(draft) => send(this, draft, cx),
                Err(error) => {
                    if let Some(session) = &mut this.session {
                        session.note(error);
                    }
                    this.after_change(cx);
                }
            });
        })
        .detach();
    }

    /// A saved image's thumbnail, loading it when not known yet.
    fn preview_of(
        &mut self,
        attachment: &Attachment,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Image>> {
        let Attachment::Image { file, .. } = attachment else {
            return None;
        };
        if let Some(image) = self.attachments.previews.get(file) {
            return Some(image.clone());
        }
        self.load_preview(attachment.clone(), cx);
        None
    }

    /// Loads thumbnails of images in sent messages (from the changed items).
    pub(super) fn load_previews(&mut self, attachments: Vec<Attachment>, cx: &mut Context<Self>) {
        for attachment in attachments {
            self.load_preview(attachment, cx);
        }
    }

    fn load_preview(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        let Attachment::Image {
            file, media_type, ..
        } = &attachment
        else {
            return;
        };
        if self.attachments.previews.contains_key(file)
            || !self.attachments.loading.insert(file.clone())
        {
            return;
        }
        let (file, format) = (file.clone(), format_of(media_type));
        let chat = self.chat_id();
        cx.spawn(async move |this, cx| {
            let bytes = cx
                .background_spawn(async move {
                    prepare::image_bytes(&folder(chat.as_deref()).ok()?, &attachment)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.attachments.loading.remove(&file);
                let Some(bytes) = bytes else {
                    return;
                };
                let image = Arc::new(Image::from_bytes(format, bytes));
                for pending in &mut this.attachments.pending {
                    if matches!(&pending.state, AttachState::Ready(Attachment::Image { file: f, .. }) if *f == file)
                    {
                        pending.preview = Some(image.clone());
                    }
                }
                this.attachments.previews.insert(file, image);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_lightbox(&mut self, image: Arc<Image>, cx: &mut Context<Self>) {
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        self.attachments.lightbox = Some(image);
        self.attachments.lightbox_presence = Some(presence);
        cx.notify();
    }

    pub(super) fn close_lightbox(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(presence) = &mut self.attachments.lightbox_presence else {
            return false;
        };
        presence.set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
        true
    }

    /// A badge clicked in the composer: its image full size.
    pub(super) fn badge_clicked(&mut self, event: &InlineTokenClickEvent, cx: &mut Context<Self>) {
        if let Some(image) = id_of(event.token())
            .and_then(|id| self.pending(id))
            .and_then(|pending| pending.preview.clone())
        {
            self.open_lightbox(image, cx);
        }
    }

    /// One badge in the composer's text: thumbnail (or spinner, warning, file icon) and name.
    pub(super) fn badge(&self, context: &InlineTokenContext) -> AnyElement {
        let pending = id_of(context.token()).and_then(|id| self.pending(id));
        let failed = pending.is_some_and(|p| matches!(p.state, AttachState::Failed(_)));
        let size = context.line_height() - px(6.);
        let leading = match pending {
            Some(Pending {
                preview: Some(image),
                ..
            }) => img(image.clone())
                .size(size)
                .rounded(px(3.))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            Some(Pending {
                state: AttachState::Preparing,
                ..
            }) => Spinner::new().into_any_element(),
            _ if failed => icon(IconName::TriangleAlert)
                .size(size)
                .text_color(t::red())
                .into_any_element(),
            Some(Pending { image: false, .. }) => {
                icon(IconName::File).size(size).into_any_element()
            }
            _ => icon(IconName::File).size(size).into_any_element(),
        };
        let border = if failed {
            t::red()
        } else if context.is_selected() {
            t::accent()
        } else {
            t::accent().opacity(0.14)
        };
        row()
            .h(context.line_height())
            .max_w(context.available_width())
            .gap(px(4.))
            .px(px(4.))
            .rounded(px(5.))
            .border_1()
            .border_color(border)
            .bg(t::accent().opacity(if context.is_selected() { 0.2 } else { 0.08 }))
            .text_color(t::text())
            .cursor_pointer()
            .child(leading)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(context.token().label().clone()),
            )
            .into_any_element()
    }

    /// Why a mentioned attachment can't be sent, under the message field.
    pub(super) fn attachment_errors(&self, cx: &App) -> Option<Div> {
        let errors: Vec<String> = self
            .mentioned(cx)
            .into_iter()
            .filter_map(|id| {
                let pending = self.pending(id)?;
                match &pending.state {
                    AttachState::Failed(error) => Some(format!("{}: {error}", pending.text)),
                    _ => None,
                }
            })
            .collect();
        (!errors.is_empty()).then(|| {
            column()
                .px(px(4.))
                .gap(px(2.))
                .text_size(px(12.))
                .text_color(t::red())
                .children(errors.into_iter().map(|error| div().child(error)))
        })
    }

    /// The image full size over the chat; click or Esc closes it.
    pub(super) fn lightbox(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let now = Instant::now();
        let presence = self.attachments.lightbox_presence.as_ref()?;
        let progress = presence.progress(now);
        let animating = presence.is_animating(now);
        motion::request_frame(window, animating);
        if progress <= 0. && !animating {
            self.attachments.lightbox = None;
            self.attachments.lightbox_presence = None;
            return None;
        }
        let image = self.attachments.lightbox.clone()?;
        let close = icon_button("chat-lightbox-close", IconName::Close, "Close (Esc)")
            .size(px(28.))
            .min_w(px(28.))
            .max_w(px(28.))
            .min_h(px(28.))
            .max_h(px(28.))
            .rounded(px(14.))
            .bg(t::elevated())
            .border_1()
            .border_color(t::control_border())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_lightbox(cx);
            }));
        Some(
            div()
                .id("chat-lightbox")
                .absolute()
                .inset_0()
                .child(crate::ui::components::modal::modal_backdrop(progress))
                // A click anywhere closes it, like Quick Look.
                .child(
                    column()
                        .id("chat-lightbox-image")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .p(px(40.))
                        .opacity(progress)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_lightbox(cx);
                        }))
                        // The image fits the chat, however large it is.
                        .child(
                            column()
                                .flex_1()
                                .min_h_0()
                                .min_w_0()
                                .overflow_hidden()
                                .child(crate::ui::components::media::fitted_image(image)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(10.))
                        .right(px(10.))
                        .opacity(progress)
                        .child(close),
                ),
        )
    }

    /// Over the whole chat: shows while files are dragged over it, attaches them on drop.
    /// Without listeners of its own otherwise, it lets the chat below work as usual.
    pub(super) fn drop_target(&self, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .inset(px(6.))
            .rounded(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.))
            .text_color(t::text())
            .opacity(0.)
            .drag_over::<ExternalPaths>(|style, _, _, _| {
                style
                    .opacity(1.)
                    .border_2()
                    .border_color(t::accent().opacity(0.6))
                    .bg(t::accent().opacity(0.06))
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.attach_paths(paths.paths().to_vec(), cx)
            }))
            .child("Drop to attach")
    }
}

/// The badge's name: the label without brackets.
fn badge_name(attachment: &Attachment, mention: &str) -> String {
    let inner = mention.trim_start_matches('[').trim_end_matches(']');
    match attachment {
        Attachment::Path { path, .. } if inner == path.display().to_string() => name_of(path),
        _ => inner.to_owned(),
    }
}

/// A file or folder sent along: its icon and name, the path in a tooltip.
fn path_chip(id: impl Into<ElementId>, name: &str, path: &Path) -> Stateful<Div> {
    let tooltip: SharedString = path.display().to_string().into();
    row()
        .id(id)
        .min_w_0()
        .max_w(px(240.))
        .h(px(28.))
        .gap(px(6.))
        .px(px(8.))
        .rounded(px(8.))
        .border_1()
        .border_color(t::control_border())
        .bg(t::bg())
        .text_size(px(12.))
        .text_color(t::text())
        .child(match crate::ui::file_icons::for_path(path) {
            Some(asset) => img(asset).size(px(14.)).flex_shrink_0().into_any_element(),
            None => icon(if path.extension().is_none() {
                IconName::Folder
            } else {
                IconName::File
            })
            .into_any_element(),
        })
        .child(div().min_w_0().truncate().child(name.to_owned()))
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
}

/// Images and files sent with a message, under its text.
pub(super) fn sent_attachments(
    view: &Entity<ChatView>,
    this: &ChatView,
    key: u64,
    attachments: &[Attachment],
) -> Option<Div> {
    if attachments.is_empty() {
        return None;
    }
    Some(
        row()
            .flex_wrap()
            .justify_end()
            .gap(px(6.))
            .children(attachments.iter().enumerate().map(|(index, attachment)| {
                let id = ElementId::Name(format!("chat-sent-{key}-{index}").into());
                match attachment {
                    Attachment::Image { file, .. } => {
                        let preview = this.attachments.previews.get(file).cloned();
                        let view = view.downgrade();
                        div()
                            .id(id)
                            .size(px(SENT_THUMB))
                            .rounded(px(10.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(t::control_border())
                            .bg(t::input_bg())
                            .children(
                                preview.clone().map(|image| {
                                    img(image).size_full().object_fit(ObjectFit::Cover)
                                }),
                            )
                            .when_some(preview, |div, image| {
                                div.cursor_pointer().on_click(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.open_lightbox(image.clone(), cx)
                                    });
                                })
                            })
                            .keyboard("Show the image")
                            .into_any_element()
                    }
                    Attachment::Path { path, .. } => {
                        path_chip(id, &name_of(path), path).into_any_element()
                    }
                }
            })),
    )
}

/// Text as Markdown that reads literally (punctuation escaped, line breaks kept).
fn literal(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\n' => out.push_str("  \n"),
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
}

/// A sent message whose text mentions its attachments, as Markdown with each mention a
/// `canopy-attachment:<index>` link (drawn as a badge); `None` without mentions.
pub(super) fn badge_markdown(text: &str, attachments: &[Attachment]) -> Option<String> {
    let mut out = String::new();
    let mut from = 0;
    for (index, attachment) in attachments.iter().enumerate() {
        let Some(mention) = attachment.label() else {
            continue;
        };
        let Some(at) = text[from..].find(mention).map(|at| from + at) else {
            continue;
        };
        literal(&text[from..at], &mut out);
        out.push('[');
        literal(&badge_name(attachment, mention), &mut out);
        out.push_str(&format!(
            "]({}{index})",
            crate::ui::markdown::attachment_badge::SCHEME
        ));
        from = at + mention.len();
    }
    if from == 0 {
        return None;
    }
    literal(&text[from..], &mut out);
    Some(out)
}

/// Whether the message text mentions this attachment (it then shows as a badge there).
pub(super) fn mentioned_in(text: &str, attachment: &Attachment) -> bool {
    attachment.label().is_some_and(|label| text.contains(label))
}

/// Badges of sent messages: the image's thumbnail (click: full size) or the file's icon.
pub(super) fn badge_factory(view: WeakEntity<ChatView>) -> super::rows::BadgeFactory {
    std::rc::Rc::new(move |attachments: &[Attachment]| {
        let attachments = attachments.to_vec();
        let view = view.clone();
        Arc::new(move |index, height, _, cx| {
            let Some(attachment) = attachments.get(index) else {
                return div().into_any_element();
            };
            let name = attachment
                .label()
                .map(|mention| badge_name(attachment, mention))
                .unwrap_or_default();
            let preview = match attachment {
                Attachment::Image { file, .. } => view
                    .upgrade()
                    .and_then(|view| view.read(cx).attachments.previews.get(file).cloned()),
                Attachment::Path { .. } => None,
            };
            let key = match attachment {
                Attachment::Image { file, .. } => file.clone(),
                Attachment::Path { path, .. } => path.display().to_string(),
            };
            let tooltip: SharedString = match attachment {
                Attachment::Image { .. } => name.clone(),
                Attachment::Path { path, .. } => path.display().to_string(),
            }
            .into();
            let size = height - px(6.);
            let view = view.clone();
            row()
                .id(ElementId::Name(format!("chat-badge-{key}-{index}").into()))
                .h(height)
                .gap(px(4.))
                .px(px(4.))
                .rounded(px(5.))
                .border_1()
                .border_color(t::accent().opacity(0.14))
                .bg(t::accent().opacity(0.08))
                .hover(|style| style.bg(t::accent().opacity(0.14)))
                .text_color(t::text())
                .child(match (&preview, attachment) {
                    (Some(image), _) => img(image.clone())
                        .size(size)
                        .rounded(px(3.))
                        .object_fit(ObjectFit::Cover)
                        .into_any_element(),
                    (None, Attachment::Path { path, .. }) => {
                        match crate::ui::file_icons::for_path(path) {
                            Some(asset) => img(asset).size(size).into_any_element(),
                            None => icon(IconName::File).size(size).into_any_element(),
                        }
                    }
                    (None, _) => icon(IconName::File).size(size).into_any_element(),
                })
                .child(div().min_w_0().truncate().child(name))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .when_some(preview, |badge, image| {
                    badge.cursor_pointer().on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = view.update(cx, |this, cx| this.open_lightbox(image.clone(), cx));
                    })
                })
                .into_any_element()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn mentions_become_badge_links_and_the_rest_reads_literally() {
        let image = Attachment::Image {
            file: "a.png".into(),
            media_type: "image/png".into(),
            width: 1,
            height: 1,
            data: None,
            label: Some("[shot.png (2 KB)]".into()),
        };
        let file = Attachment::Path {
            path: "/tmp/notes.txt".into(),
            label: Some("[/tmp/notes.txt]".into()),
        };
        let text = "what is *in* [shot.png (2 KB)]\nand [/tmp/notes.txt]?";
        let markdown = badge_markdown(text, &[image.clone(), file]).expect("mentions");
        assert_eq!(
            markdown,
            "what is \\*in\\* [shot\\.png \\(2 KB\\)](canopy-attachment:0)  \nand [notes\\.txt](canopy-attachment:1)\\?"
        );
        assert!(badge_markdown("no mention", &[image]).is_none());
        assert_eq!(size_label(840), "840 B");
        assert_eq!(size_label(120 * 1024), "120 KB");
    }
}
