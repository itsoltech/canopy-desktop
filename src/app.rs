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
            // Development builds: the layout inspector picks what is visible.
            #[cfg(any(debug_assertions, feature = "dev-inspector"))]
            crate::ui::dev_inspector::init(cx);
            canopy_desktop::motion::init_system_policy();
            crate::ui::editor::init(cx);
            crate::ui::terminal::init(cx);
            crate::ui::chat::init(cx);
            #[cfg(feature = "frame-profile")]
            crate::profiling::init(cx);
            crate::ui::theme::init(cx);
            crate::ui::components::modal::init_prompts(cx);
            crate::app_state::AppState::init(cx);
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                    "../assets/fonts/JetBrainsMono-Regular.ttf"
                ))])
                .expect("embedded font");
            cx.on_action(|_: &Quit, cx| crate::app_state::AppState::quit(cx));
            // ⌘Q is a customizable command; the menu follows its current shortcut.
            crate::ui::keymap::set_menus(
                || {
                    vec![Menu {
                        name: "Canopy".into(),
                        disabled: false,
                        items: vec![MenuItem::action("Quit Canopy", Quit)],
                    }]
                },
                cx,
            );

            // Diagnostic preview keeps only the overlay visible for window captures.
            let notch_preview = cfg!(any(target_os = "macos", target_os = "windows"))
                && std::env::var_os("CANOPY_NOTCH_PREVIEW").is_some();
            let (display_id, window_bounds) = crate::ui::window_geometry::placement(cx);
            let (min_width, min_height) = crate::ui::window_geometry::MIN_SIZE;
            let options = WindowOptions {
                show: !notch_preview,
                window_bounds: Some(window_bounds),
                display_id,
                window_min_size: Some(size(px(min_width), px(min_height))),
                titlebar: Some(crate::ui::components::titlebar_options("Canopy")),
                ..Default::default()
            };
            let main = match cx.open_window(options, |window, cx| {
                window.on_window_should_close(cx, |_, cx| {
                    crate::app_state::AppState::quit(cx);
                    false
                });
                crate::ui::theme::bind(window, cx);
                crate::ui::window_geometry::track(window, cx);
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
            // The overlay opens and closes with the Notch setting.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            crate::ui::notch::bind(main, notch_preview, cx);
            cx.on_window_closed(move |cx, _| {
                if cx
                    .windows()
                    .iter()
                    .all(|window| is_notch(window.window_id(), cx))
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

/// The notch overlay alone doesn't keep Canopy running.
fn is_notch(window: WindowId, cx: &App) -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return crate::ui::notch::is_overlay(window, cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window, cx);
        false
    }
}
