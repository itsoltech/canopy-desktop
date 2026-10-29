//! Opens the main window where it was last time and saves its frame as it moves.
use canopy_desktop::state::window_geometry::{Rect, WindowGeometry, WindowState};
use gpui_kit::*;
use std::{path::PathBuf, time::Duration};

pub(crate) const MIN_SIZE: (f32, f32) = (800., 500.);
const DEFAULT_SIZE: (f32, f32) = (1423., 892.);
/// Moves and resizes arrive per frame; the file is written once they settle.
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The display and frame for the main window: the saved one when it still fits a
/// connected display, otherwise the saved size centered on the primary display.
pub(crate) fn placement(cx: &App) -> (Option<DisplayId>, WindowBounds) {
    let saved = canopy_desktop::platform::directories::data_dir()
        .ok()
        .and_then(|dir| WindowGeometry::load(&dir));
    let Some(saved) = saved else {
        return (None, centered(None, DEFAULT_SIZE, cx));
    };
    let display = saved.display.as_ref().and_then(|saved| {
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().is_ok_and(|id| id.to_string() == *saved))
    });
    let restored = display.is_some();
    let target = display.or_else(|| cx.primary_display());
    let area = target
        .as_ref()
        .map_or((f32::INFINITY, f32::INFINITY), |display| {
            let size = display.bounds().size;
            (f32::from(size.width), f32::from(size.height))
        });
    let id = target.as_ref().map(|display| display.id());
    let fitted = restored.then(|| saved.fit(area, MIN_SIZE)).flatten();
    let Some(Rect {
        x,
        y,
        width,
        height,
    }) = fitted
    else {
        let size = saved.size_on(area, MIN_SIZE).unwrap_or(DEFAULT_SIZE);
        return (id, centered(id, size, cx));
    };
    let bounds = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
    let bounds = match saved.state {
        WindowState::Normal => WindowBounds::Windowed(bounds),
        WindowState::Maximized => WindowBounds::Maximized(bounds),
        WindowState::Fullscreen => WindowBounds::Fullscreen(bounds),
    };
    (id, bounds)
}

fn centered(display: Option<DisplayId>, (width, height): (f32, f32), cx: &App) -> WindowBounds {
    WindowBounds::Windowed(Bounds::centered(display, size(px(width), px(height)), cx))
}

/// Saves the main window's frame until Canopy quits, the last change included.
pub(crate) fn track(window: &mut Window, cx: &mut App) {
    let Ok(dir) = canopy_desktop::platform::directories::data_dir() else {
        return;
    };
    let tracker = cx.new(|cx| Tracker {
        dir,
        last: None,
        pending: None,
        _save: None,
        _subscriptions: [
            cx.observe_window_bounds(window, |this: &mut Tracker, window, cx| {
                this.changed(window, cx)
            }),
            cx.on_app_quit(|this: &mut Tracker, cx| this.flush(cx)),
        ],
    });
    cx.set_global(Tracked { _tracker: tracker });
}

/// Keeps the tracker alive for the app's lifetime.
struct Tracked {
    _tracker: Entity<Tracker>,
}
impl Global for Tracked {}

struct Tracker {
    dir: PathBuf,
    last: Option<WindowGeometry>,
    pending: Option<WindowGeometry>,
    _save: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl Tracker {
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (bounds, state) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, WindowState::Normal),
            WindowBounds::Maximized(bounds) => (bounds, WindowState::Maximized),
            WindowBounds::Fullscreen(bounds) => (bounds, WindowState::Fullscreen),
        };
        let geometry = WindowGeometry {
            display: window
                .display(cx)
                .and_then(|display| display.uuid().ok())
                .map(|id| id.to_string()),
            bounds: Rect {
                x: bounds.origin.x.into(),
                y: bounds.origin.y.into(),
                width: bounds.size.width.into(),
                height: bounds.size.height.into(),
            },
            state,
        };
        if self.last.as_ref() == Some(&geometry) {
            return;
        }
        self.last = Some(geometry.clone());
        self.pending = Some(geometry);
        self._save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            if let Ok(write) = this.update(cx, |this, cx| this.flush(cx)) {
                write.await;
            }
        }));
    }

    fn flush(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(geometry) = self.pending.take() else {
            return Task::ready(());
        };
        let dir = self.dir.clone();
        cx.background_executor().spawn(async move {
            if let Err(error) = geometry.save(&dir) {
                eprintln!("Window position was not saved: {error}");
            }
        })
    }
}
