//! A PTY child must not inherit a background executor's blocked signal mask.
use alacritty_terminal::{event::WindowSize, tty};
use std::io;

pub(super) fn pty(options: &tty::Options, size: WindowSize) -> io::Result<tty::Pty> {
    // Alacritty resets signal dispositions in pre_exec, but leaves the mask
    // inherited from this thread. macOS dispatch workers block SIGWINCH (and
    // other job-control signals). Unlike a shell, a direct TUI may retain it.
    #[cfg(unix)]
    let _signals = SpawnSignals::unblock()?;
    tty::new(options, size, 0)
}

#[cfg(unix)]
struct SpawnSignals {
    previous: libc::sigset_t,
    // A mask belongs to the calling thread; never restore it on another thread.
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(unix)]
impl SpawnSignals {
    fn unblock() -> io::Result<Self> {
        // SAFETY: both masks are valid for the call. pthread_sigmask changes
        // only this thread, and its old mask is restored on every exit path.
        unsafe {
            let mut empty = std::mem::zeroed();
            libc::sigemptyset(&mut empty);
            let mut previous = std::mem::zeroed();
            let error = libc::pthread_sigmask(libc::SIG_SETMASK, &empty, &mut previous);
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            Ok(Self {
                previous,
                _thread: std::marker::PhantomData,
            })
        }
    }
}

#[cfg(unix)]
impl Drop for SpawnSignals {
    fn drop(&mut self) {
        // SAFETY: this guard cannot move to another thread. The saved mask and
        // operation are valid; null means we do not need the replaced mask.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.previous, std::ptr::null_mut());
        }
    }
}
