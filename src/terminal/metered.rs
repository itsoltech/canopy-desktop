//! Alacritty's input channel and write list are unbounded. Bytes queued for a PTY
//! are counted here until its writer actually accepts them, so a shell that stops
//! reading cannot make one session (or all sessions) buffer unlimited input.
use alacritty_terminal::{
    event::{OnResize, WindowSize},
    tty::{ChildEvent, EventedPty, EventedReadWrite},
};
use polling::{Event, PollMode, Poller};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Queued, not yet written input allowed per session.
pub const SESSION_QUEUE_BYTES: usize = super::session::INPUT_LIMIT_BYTES;
/// Queued, not yet written input allowed across all sessions of the process.
pub const GLOBAL_QUEUE_BYTES: usize = 16 * SESSION_QUEUE_BYTES;
static QUEUED: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputRefused {
    NotRunning,
    TooLarge,
    QueueFull,
}
impl std::fmt::Display for InputRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotRunning => "The terminal process is not running.",
            Self::TooLarge => "Input is too large to send to this terminal.",
            Self::QueueFull => "The terminal is not reading input yet. Try again shortly.",
        })
    }
}

/// (queued bytes, closed). Closed after the I/O thread ended: nothing is written
/// anymore, so the remainder is returned to the global budget once.
#[derive(Default)]
pub(super) struct InputBudget(Mutex<(usize, bool)>);
impl InputBudget {
    /// `limited` is false only for the emulator's own replies (DA, colors, size),
    /// which are counted but cannot be refused without breaking the protocol.
    pub(super) fn reserve(&self, bytes: usize, limited: bool) -> Result<(), InputRefused> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.1 {
            return Err(InputRefused::NotRunning);
        }
        if limited && state.0.saturating_add(bytes) > SESSION_QUEUE_BYTES {
            return Err(InputRefused::QueueFull);
        }
        QUEUED
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                queued
                    .checked_add(bytes)
                    .filter(|total| !limited || *total <= GLOBAL_QUEUE_BYTES)
            })
            .map_err(|_| InputRefused::QueueFull)?;
        state.0 += bytes;
        Ok(())
    }
    pub(super) fn written(&self, bytes: usize) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let bytes = bytes.min(state.0);
        state.0 -= bytes;
        QUEUED.fetch_sub(bytes, Ordering::AcqRel);
    }
    pub(super) fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.1 = true;
        QUEUED.fetch_sub(std::mem::take(&mut state.0), Ordering::AcqRel);
    }
    #[cfg(test)]
    fn queued(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).0
    }
}

/// Wraps the PTY so every accepted write releases its budget.
pub(super) struct Metered<P>(MeteredWriter<P>);
pub(super) struct MeteredWriter<P> {
    pty: P,
    budget: Arc<InputBudget>,
}
impl<P> Metered<P> {
    pub(super) fn new(pty: P, budget: Arc<InputBudget>) -> Self {
        Self(MeteredWriter { pty, budget })
    }
}
impl<P: EventedReadWrite> io::Write for MeteredWriter<P> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.pty.writer().write(bytes)?;
        self.budget.written(written);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.pty.writer().flush()
    }
}
impl<P: EventedReadWrite> EventedReadWrite for Metered<P> {
    type Reader = P::Reader;
    type Writer = MeteredWriter<P>;
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: the owned inner PTY outlives its registration, as before wrapping.
        unsafe { self.0.pty.register(poll, event, mode) }
    }
    fn reregister(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.0.pty.reregister(poll, event, mode)
    }
    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.0.pty.deregister(poll)
    }
    fn reader(&mut self) -> &mut Self::Reader {
        self.0.pty.reader()
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.0
    }
}
impl<P: EventedPty> EventedPty for Metered<P> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.0.pty.next_child_event()
    }
}
impl<P: OnResize> OnResize for Metered<P> {
    fn on_resize(&mut self, size: WindowSize) {
        self.0.pty.on_resize(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_input_is_bounded_until_written_and_released_on_close() {
        let budget = InputBudget::default();
        budget.reserve(SESSION_QUEUE_BYTES, true).unwrap();
        assert_eq!(budget.reserve(1, true), Err(InputRefused::QueueFull));
        // Emulator replies are counted even when user input is refused.
        budget.reserve(3, false).unwrap();
        budget.written(4);
        budget.reserve(1, true).unwrap();
        budget.close();
        assert_eq!(budget.queued(), 0);
        assert_eq!(budget.reserve(1, false), Err(InputRefused::NotRunning));
    }
}
