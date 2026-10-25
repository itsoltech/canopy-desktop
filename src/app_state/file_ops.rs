//! The host's one reservation gate for Canopy's own writes (`files::Gate`): editor
//! Save, an agent's file write and the remote file and Git services share it, so one
//! file or checkout is changed by one Canopy operation at a time. It doesn't lock other
//! programs. The remote server gets this same gate (`ResourceLocks::with_gate`).
use canopy_desktop::files::Gate;

#[derive(Clone, Default)]
pub struct FileGate(Gate);
impl gpui_kit::Global for FileGate {}

/// The app's gate, created on first use.
pub fn gate(cx: &mut gpui_kit::App) -> Gate {
    cx.default_global::<FileGate>().0.clone()
}
