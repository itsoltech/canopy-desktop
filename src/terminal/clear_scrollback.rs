//! Correct macOS clear's E3 + home + ED2 ordering without changing ordinary ED2.
//! The reader forwards bytes immediately; only the exact full-clear sequence adds
//! a final E3, after Alacritty's ED2 has moved the old viewport into scrollback.
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event, PollMode, Poller};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read},
    sync::Arc,
};
const CLEAR: &[u8] = b"\x1b[3J\x1b[H\x1b[2J";
const ERASE_SAVED: &[u8] = b"\x1b[3J";

pub struct ClearReader<R> {
    source: R,
    pending: VecDeque<u8>,
    matched: usize,
}
impl<R> ClearReader<R> {
    fn new(source: R) -> Self {
        Self {
            source,
            pending: VecDeque::new(),
            matched: 0,
        }
    }
}
impl<R: Read> Read for ClearReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if !self.pending.is_empty() {
            let n = out.len().min(self.pending.len());
            for b in &mut out[..n] {
                *b = self.pending.pop_front().unwrap();
            }
            return Ok(n);
        }
        let n = self.source.read(out)?;
        if self.matched == 0 && !out[..n].contains(&0x1b) {
            return Ok(n);
        }
        let mut matches = Vec::new();
        for (i, &byte) in out[..n].iter().enumerate() {
            if byte == CLEAR[self.matched] {
                self.matched += 1;
            } else {
                self.matched = usize::from(byte == CLEAR[0]);
            }
            if self.matched == CLEAR.len() {
                matches.push(i + 1);
                self.matched = 0;
            }
        }
        if matches.is_empty() {
            return Ok(n);
        }
        let mut expanded = Vec::with_capacity(n + matches.len() * ERASE_SAVED.len());
        let mut start = 0;
        for end in matches {
            expanded.extend_from_slice(&out[start..end]);
            expanded.extend_from_slice(ERASE_SAVED);
            start = end;
        }
        expanded.extend_from_slice(&out[start..n]);
        let used = out.len().min(expanded.len());
        out[..used].copy_from_slice(&expanded[..used]);
        self.pending.extend(&expanded[used..]);
        Ok(used)
    }
}
pub struct ClearPty {
    inner: tty::Pty,
    reader: ClearReader<File>,
}
impl ClearPty {
    pub fn new(inner: tty::Pty) -> io::Result<Self> {
        let reader = ClearReader::new(inner.file().try_clone()?);
        Ok(Self { inner, reader })
    }
}
impl EventedReadWrite for ClearPty {
    type Reader = ClearReader<File>;
    type Writer = File;
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: the owned inner PTY outlives its registration, as before wrapping.
        unsafe { self.inner.register(poll, event, mode) }
    }
    fn reregister(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.inner.reregister(poll, event, mode)
    }
    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.inner.deregister(poll)
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        self.inner.writer()
    }
}
impl EventedPty for ClearPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}
impl alacritty_terminal::event::OnResize for ClearPty {
    fn on_resize(&mut self, size: alacritty_terminal::event::WindowSize) {
        self.inner.on_resize(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_clear_is_fixed_across_every_read_boundary() {
        let input = [b"before".as_slice(), CLEAR, b"after"].concat();
        let expected = [b"before".as_slice(), CLEAR, ERASE_SAVED, b"after"].concat();
        for chunk in 1..=input.len() {
            let mut reader = ClearReader::new(input.as_slice());
            let mut out = vec![];
            let mut buffer = vec![0; chunk];
            loop {
                let n = reader.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buffer[..n]);
            }
            assert_eq!(out, expected, "chunk {chunk}");
        }
    }
    #[test]
    fn ordinary_redraw_and_standard_clear_are_unchanged() {
        let input = b"\x1b[H\x1b[2Jtext\x1b[3Jmore\x1b[2J\x1b[3J\x1b[3J\x1b[Hnew output\x1b[2J";
        let mut reader = ClearReader::new(input.as_slice());
        let mut out = vec![];
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, input);
    }
}
