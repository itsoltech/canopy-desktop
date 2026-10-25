use super::*;
/// Device-local preferences are copied into each window. Saving a default never
/// changes another open window and never calls the host Settings service.
pub(super) struct DevicePreferences {
    host: Arc<HostClient>,
    runtime: Arc<tokio::runtime::Runtime>,
    zoom: u16,
    error: Option<String>,
}
impl DevicePreferences {
    pub(super) fn new(host: Arc<HostClient>, runtime: Arc<tokio::runtime::Runtime>) -> Self {
        let zoom = host
            .snapshot()
            .record
            .view("device-preferences")
            .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
            .and_then(|v| v["zoom"].as_u64())
            .filter(|v| (75..=150).contains(v))
            .unwrap_or(100) as u16;
        Self {
            host,
            runtime,
            zoom,
            error: None,
        }
    }
    pub(super) fn apply(&self, window: &mut Window, cx: &App) {
        // Font size is the physical anchor for GPUI's rem-based layout scale.
        window.set_rem_size(
            gpui_kit::component::Theme::global(cx).font_size * (self.zoom as f32 / 100.),
        );
    }
    fn set_zoom(&mut self, zoom: u16, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom = zoom;
        self.apply(window, cx);
        window.refresh();
        cx.notify();
        let host = self.host.clone();
        let value = serde_json::json!({"zoom":zoom}).to_string();
        let revision = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let task = self.runtime.spawn_blocking(move || {
            host.save_view_version("device-preferences".into(), value, revision)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await.map_err(|e| e.to_string()).and_then(|r| r);
            let _ = this.update(cx, |s, cx| {
                s.error = result.err();
                cx.notify();
            });
        })
        .detach();
    }
}
impl Render for DevicePreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column().gap_2().child("This device").child("Zoom applies to this window and becomes the local default for this host. Other windows keep their current zoom.")
            .child(row().gap_2().children([75,90,100,110,125,150].into_iter().map(|zoom|selection_button(SharedString::from(format!("zoom:{zoom}")),format!("{zoom}%"),self.zoom==zoom).on_click(cx.listener(move|s,_,w,cx|s.set_zoom(zoom,w,cx))))))
            .children(self.error.clone().map(|e|div().text_color(t::red()).child(format!("Could not save device preference: {e}"))))
    }
}
