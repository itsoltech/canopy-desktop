use gpui_kit::component::Root;
use gpui_kit::*;

actions!(canopy, [Quit]);

pub fn run() {
    let http = match canopy_desktop::http::new_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    gpui_kit::application()
        .with_http_client(http)
        .with_assets(crate::ui::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            canopy_desktop::motion::init_system_policy();
            crate::ui::editor::init(cx);
            crate::ui::terminal::init(cx);
            #[cfg(feature = "frame-profile")]
            crate::profiling::init(cx);
            crate::ui::theme::init(cx);
            crate::app_state::AppState::init(cx);
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                    "../assets/fonts/JetBrainsMono-Regular.ttf"
                ))])
                .expect("embedded font");
            cx.on_action(|_: &Quit, cx| crate::app_state::AppState::quit(cx));
            #[cfg(target_os = "macos")]
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus(vec![Menu {
                name: "Canopy".into(),
                disabled: false,
                items: vec![MenuItem::action("Quit Canopy", Quit)],
            }]);

            // Diagnostic preview keeps only the overlay visible for window captures.
            let notch_preview = cfg!(any(target_os = "macos", target_os = "windows"))
                && std::env::var_os("CANOPY_NOTCH_PREVIEW").is_some();
            let options = WindowOptions {
                show: !notch_preview,
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1423.), px(892.)),
                    cx,
                ))),
                window_min_size: Some(size(px(800.), px(500.))),
                titlebar: Some(crate::ui::components::titlebar_options("Canopy")),
                ..Default::default()
            };
            let main = match cx.open_window(options, |window, cx| {
                window.on_window_should_close(cx, |_, cx| {
                    crate::app_state::AppState::quit(cx);
                    false
                });
                let view = cx.new(|cx| crate::ui::Workspace::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            }) {
                Ok(main) => main,
                Err(error) => {
                    eprintln!("Failed to open Canopy: {error:#}");
                    cx.quit();
                    return;
                }
            };
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let overlay = match crate::ui::notch::open(main, cx) {
                Ok(handle) => Some(handle.window_id()),
                Err(error) => {
                    eprintln!("Failed to open notch: {error:#}");
                    None
                }
            };
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let overlay: Option<WindowId> = None;
            cx.on_window_closed(move |cx, _| {
                if cx
                    .windows()
                    .iter()
                    .all(|window| Some(window.window_id()) == overlay)
                {
                    crate::app_state::AppState::quit(cx);
                }
            })
            .detach();
            cx.activate(true);
            if !notch_preview {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            }
        });
}
