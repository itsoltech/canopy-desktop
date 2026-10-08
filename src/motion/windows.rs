pub(super) fn reduced_motion() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut enabled = 1i32;
    // SAFETY: enabled is writable storage for the documented BOOL result.
    if unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        )
    } == 0
    {
        None
    } else {
        Some(enabled == 0)
    }
}
