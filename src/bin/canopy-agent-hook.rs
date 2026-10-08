fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--canopy-hook-version")
    {
        println!("{}", canopy_desktop::agents::launch::HOOK_HELPER_VERSION);
        return;
    }
    canopy_desktop::agents::relay::forward();
}
