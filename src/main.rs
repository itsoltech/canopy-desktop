#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod app_state;
#[cfg(feature = "frame-profile")]
mod profiling;
mod ui;

fn main() {
    if let Some(argument) = std::env::args_os().nth(1) {
        if argument == "--agent-hook" {
            canopy_desktop::agents::relay::forward();
            return;
        }
        if argument == "--canopy-hook-version" {
            println!("{}", canopy_desktop::agents::launch::HOOK_HELPER_VERSION);
            return;
        }
    }
    // SAFETY: first operation, before GPUI and its worker threads are created.
    unsafe {
        canopy_desktop::terminal::environment::prepare_desktop_environment();
        // libgit2 transport settings are C globals: configure only before threads.
        git2::opts::set_server_connect_timeout_in_milliseconds(15_000)
            .expect("configure Git connect timeout");
        git2::opts::set_server_timeout_in_milliseconds(30_000)
            .expect("configure Git transport timeout");
    }
    app::run();
}
