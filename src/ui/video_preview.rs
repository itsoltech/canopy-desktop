use super::components::media::*;
#[cfg(target_os = "macos")]
use super::video_native::Player;
use super::{components::*, theme as t};
use canopy_desktop::state::workspace::Pane;
use gpui_kit::{base::Disableable, *};
use std::{cell::Cell, rc::Rc, time::Duration};
#[cfg(target_os = "macos")]
pub struct VideoPreview {
    player: Option<Rc<Player>>,
    error: Option<String>,
    loading: bool,
    playing: bool,
    time: f64,
    duration: f64,
    visible: bool,
    obscured: bool,
    show: Rc<Cell<bool>>,
    bar: Rc<Cell<Bounds<Pixels>>>,
    task: Option<Task<()>>,
    ticker: Option<Task<()>>,
}
#[cfg(target_os = "macos")]
impl VideoPreview {
    pub fn new(pane: Pane, window: &Window, cx: &mut Context<Self>) -> Self {
        let root = pane.metadata.cwd.unwrap_or_default();
        let path = std::path::PathBuf::from(pane.metadata.resource.unwrap_or_default());
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let path = canopy_desktop::files::resolve(&root, &path)?;
                    if !path.is_file() {
                        return Err("Video file is unavailable.".into());
                    }
                    Ok(path)
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this: &mut Self, cx| {
                    match result.and_then(|path| Player::new(&path, window)) {
                        Ok(player) => {
                            this.player = Some(Rc::new(player));
                            this.tick(cx);
                        }
                        Err(e) => {
                            this.loading = false;
                            this.error = Some(e);
                        }
                    }
                    cx.notify();
                })
            });
        });
        Self {
            player: None,
            error: None,
            loading: true,
            playing: false,
            time: 0.,
            duration: 0.,
            visible: true,
            obscured: false,
            show: Rc::new(Cell::new(true)),
            bar: Rc::new(Cell::new(Bounds::default())),
            task: Some(task),
            ticker: None,
        }
    }
    pub fn visibility(&mut self, visible: bool, obscured: bool, cx: &mut Context<Self>) {
        if self.visible == visible && self.obscured == obscured {
            return;
        }
        self.visible = visible;
        self.obscured = obscured;
        let show = visible && !obscured;
        self.show.set(show);
        if !show {
            if let Some(player) = &self.player {
                player.hide();
            }
            self.playing = false;
        }
        cx.notify();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        self.ticker=Some(cx.spawn(async move|this,cx|{
            let mut tick=0;
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let keep=this.update(cx,|this,cx|{
                    let Some(player)=&this.player else{return false;};
                    let(status,time,duration,playing)=player.status();this.time=time;this.duration=duration;this.playing=playing;
                    if status==2 || (status==0&&tick>=39) {this.error=Some("This video cannot be played by the macOS decoder. Try MP4 (H.264/AAC); MKV support depends on the system codecs.".into());this.loading=false;player.hide();this.show.set(false);cx.notify();return false;}
                    this.loading=status==0;cx.notify();
                    this.loading||playing||tick<2
                }).unwrap_or(false);
                if !keep {break;}
                tick+=1;
            }
        }));
    }
    fn play(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            if self.duration > 0. && self.time >= self.duration - 0.1 {
                player.seek(0.);
            }
            self.playing = !self.playing;
            player.play(self.playing);
            self.tick(cx);
            cx.notify();
        }
    }
    fn seek(&mut self, time: f64, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            self.time = time.clamp(0., self.duration);
            player.seek(self.time);
            self.tick(cx);
            cx.notify();
        }
    }
    pub fn release(&mut self) {
        self.ticker = None;
        self.task = None;
        if let Some(player) = self.player.take() {
            player.hide();
        }
    }
}
#[cfg(target_os = "macos")]
impl Render for VideoPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = self.player.clone();
        let show = self.show.clone();
        let bar = self.bar.clone();
        let ratio = if self.duration > 0. {
            (self.time / self.duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        preview_surface()
            .p(px(12.))
            .gap(px(8.))
            .child(
                column()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .children(
                        self.loading
                            .then(|| div().text_color(t::muted()).child("Loading video…")),
                    )
                    .children(
                        self.error
                            .clone()
                            .map(|e| div().text_color(t::secondary()).child(e)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                if let Some(player) = &player {
                                    if show.get() {
                                        player.frame(bounds);
                                    } else {
                                        player.hide();
                                    }
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .child(
                media_seek_bar("video-progress", ratio)
                    .child(
                        canvas(|_, _, _| (), move |bounds, _, _, _| bar.set(bounds))
                            .absolute()
                            .inset_0(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            let b = this.bar.get();
                            let w = f32::from(b.size.width);
                            if w > 0. {
                                let fraction =
                                    (f32::from(event.position.x - b.left()) / w).clamp(0., 1.);
                                this.seek(fraction as f64 * this.duration, cx);
                            }
                        }),
                    ),
            )
            .child(playback_controls(
                button("video-back", "−10s")
                    .disabled(self.duration <= 0.)
                    .on_click(cx.listener(|this, _, _, cx| this.seek(this.time - 10., cx))),
                button("video-play", if self.playing { "Pause" } else { "Play" })
                    .disabled(self.loading || self.error.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.play(cx))),
                button("video-forward", "+10s")
                    .disabled(self.duration <= 0.)
                    .on_click(cx.listener(|this, _, _, cx| this.seek(this.time + 10., cx))),
                self.time,
                self.duration,
            ))
    }
}
