//! The host's Alacritty projection on the wire: fixed masks (never the Alacritty
//! ABI), packed RGBA, spans, and patches against what one subscriber holds.
use super::proto;
use crate::remote::sync::{ByteReservation, ReplayHub};
use crate::terminal::session::{Cell, Frame, Mode};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::Rgb;
use prost::Message;
use sha2::{Digest, Sha256};

/// Fragment payload, as for transcript items.
pub(super) const FRAGMENT_BYTES: usize = 64 * 1024;
/// Largest assembled frame a client must accept.
pub(super) const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;
/// Encoded frames above this travel as fragments (the message limit is 1 MiB).
pub(super) const INLINE_FRAME_BYTES: usize = 512 * 1024;

const MODES: [(Mode, proto::TerminalInputMode); 13] = [
    (
        Mode::APP_CURSOR,
        proto::TerminalInputMode::ApplicationCursor,
    ),
    (
        Mode::APP_KEYPAD,
        proto::TerminalInputMode::ApplicationKeypad,
    ),
    (
        Mode::BRACKETED_PASTE,
        proto::TerminalInputMode::BracketedPaste,
    ),
    (Mode::ALT_SCREEN, proto::TerminalInputMode::AlternateScreen),
    (Mode::SHOW_CURSOR, proto::TerminalInputMode::CursorVisible),
    (
        Mode::LINE_FEED_NEW_LINE,
        proto::TerminalInputMode::NewLineMode,
    ),
    (Mode::FOCUS_IN_OUT, proto::TerminalInputMode::FocusReporting),
    (
        Mode::MOUSE_REPORT_CLICK,
        proto::TerminalInputMode::MouseClick,
    ),
    (Mode::MOUSE_DRAG, proto::TerminalInputMode::MouseDrag),
    (Mode::MOUSE_MOTION, proto::TerminalInputMode::MouseMotion),
    (Mode::SGR_MOUSE, proto::TerminalInputMode::SgrMouse),
    (Mode::UTF8_MOUSE, proto::TerminalInputMode::Utf8Mouse),
    (
        Mode::ALTERNATE_SCROLL,
        proto::TerminalInputMode::AlternateScroll,
    ),
];
// INVERSE and DIM are already applied to the projected colors, so they are not
// repeated as attributes (a client would otherwise swap or dim twice).
const FLAGS: [(Flags, proto::TerminalCellFlag); 13] = [
    (Flags::BOLD, proto::TerminalCellFlag::CellBold),
    (Flags::ITALIC, proto::TerminalCellFlag::CellItalic),
    (Flags::UNDERLINE, proto::TerminalCellFlag::CellUnderline),
    (Flags::STRIKEOUT, proto::TerminalCellFlag::CellStrikeout),
    (Flags::WIDE_CHAR, proto::TerminalCellFlag::CellWide),
    (
        Flags::WIDE_CHAR_SPACER,
        proto::TerminalCellFlag::CellWideSpacer,
    ),
    (
        Flags::LEADING_WIDE_CHAR_SPACER,
        proto::TerminalCellFlag::CellLeadingWideSpacer,
    ),
    (Flags::HIDDEN, proto::TerminalCellFlag::CellHidden),
    (Flags::WRAPLINE, proto::TerminalCellFlag::CellWrapline),
    (
        Flags::DOUBLE_UNDERLINE,
        proto::TerminalCellFlag::CellDoubleUnderline,
    ),
    (Flags::UNDERCURL, proto::TerminalCellFlag::CellUndercurl),
    (
        Flags::DOTTED_UNDERLINE,
        proto::TerminalCellFlag::CellDottedUnderline,
    ),
    (
        Flags::DASHED_UNDERLINE,
        proto::TerminalCellFlag::CellDashedUnderline,
    ),
];

pub(super) fn modes(mode: Mode) -> u64 {
    MODES
        .iter()
        .filter(|(local, _)| mode.contains(*local))
        .fold(0, |bits, (_, wire)| bits | *wire as u64)
}
fn flags(flags: Flags) -> u32 {
    FLAGS
        .iter()
        .filter(|(local, _)| flags.contains(*local))
        .fold(0, |bits, (_, wire)| bits | *wire as u32)
}
fn rgba(color: Rgb) -> u32 {
    (u32::from(color.r) << 24) | (u32::from(color.g) << 16) | (u32::from(color.b) << 8) | 0xff
}
fn cell(cell: &Cell) -> proto::TerminalCell {
    let spacer = cell.flags.contains(Flags::WIDE_CHAR_SPACER);
    proto::TerminalCell {
        column: cell.column as u32,
        row: cell.row as u32,
        // The wide character to the left already covers a spacer's column.
        text: if spacer {
            String::new()
        } else {
            cell.text.clone()
        },
        foreground_rgba: rgba(cell.fg),
        background_rgba: rgba(cell.bg),
        flags: flags(cell.flags),
        column_span: if spacer { 0 } else { cell.width as u32 },
    }
}

/// What one subscriber last installed; patches are computed against it.
#[derive(Default)]
pub(super) struct Installed {
    size: Option<(u32, u32)>,
    cells: Vec<proto::TerminalCell>,
    cursor: (u32, u32),
    modes: u64,
    lost: bool,
    _reservation: Option<ByteReservation>,
}
impl Installed {
    /// The grid reader has already admitted both conversion copies and encoding.
    pub(super) fn next_bounded(
        &mut self,
        frame: Frame,
        lost: bool,
        mut reservation: ByteReservation,
    ) -> Result<Option<Encoded>, tonic::Status> {
        let Some(wire) = self.next(&frame, lost) else {
            return Ok(None);
        };
        drop(frame);
        let retained = cells_bytes(&self.cells);
        self._reservation = Some(reservation.split(retained));
        encode_reserved(wire, Some(reservation)).map(Some)
    }
    /// A complete frame for a new subscriber or after a size change; otherwise only
    /// changed cells. None when nothing the client shows changed.
    pub(super) fn next(&mut self, frame: &Frame, lost: bool) -> Option<proto::TerminalFrame> {
        let size = (frame.size.columns as u32, frame.size.rows as u32);
        let cells: Vec<_> = frame.cells.iter().map(cell).collect();
        let cursor = (frame.cursor.1 as u32, frame.cursor.0 as u32);
        let modes = modes(frame.mode);
        let complete = self.size != Some(size) || self.cells.len() != cells.len();
        let changed: Vec<_> = if complete {
            cells.clone()
        } else {
            let changed = || {
                cells
                    .iter()
                    .zip(&self.cells)
                    .filter(|(now, before)| now != before)
            };
            let mut patch = Vec::with_capacity(changed().count());
            patch.extend(changed().map(|(now, _)| now.clone()));
            patch
        };
        if !complete
            && changed.is_empty()
            && self.cursor == cursor
            && self.modes == modes
            && self.lost == lost
        {
            return None;
        }
        *self = Self {
            size: Some(size),
            cells,
            cursor,
            modes,
            lost,
            _reservation: None,
        };
        Some(proto::TerminalFrame {
            columns: size.0,
            rows: size.1,
            cells: changed,
            cursor_column: cursor.0,
            cursor_row: cursor.1,
            input_modes: modes,
            lost_scrollback: lost,
            complete,
        })
    }
}

/// Admission happens under the emulator lock, before its Frame allocation.
/// Covers raw cells/text, Installed + patch copies and the protobuf encoding.
/// The previous Installed reservation remains live until replacement succeeds.
pub(super) fn admit(
    hub: &ReplayHub,
    cells: usize,
    text: usize,
) -> Result<ByteReservation, tonic::Status> {
    let bytes = cells
        .saturating_mul(
            std::mem::size_of::<Cell>() + 2 * std::mem::size_of::<proto::TerminalCell>() + 64,
        )
        .saturating_add(text.saturating_mul(4))
        .saturating_add(1024);
    hub.reserve_assembly(bytes).map_err(|_| {
        tonic::Status::resource_exhausted(
            "Terminal projection budget is full. Resubscribe for a fresh snapshot.",
        )
    })
}
fn cells_bytes(cells: &[proto::TerminalCell]) -> usize {
    std::mem::size_of_val(cells) + cells.iter().map(|cell| cell.text.capacity()).sum::<usize>()
}
pub(super) fn page_bounded(
    frame: Frame,
    lost: bool,
    reservation: ByteReservation,
) -> Result<Encoded, tonic::Status> {
    let wire = page(&frame, lost);
    drop(frame);
    encode_reserved(wire, Some(reservation))
}

/// A read-only page: always complete, never compared with the live view.
pub(super) fn page(frame: &Frame, lost: bool) -> proto::TerminalFrame {
    Installed::default()
        .next(frame, lost)
        .expect("a first frame is complete")
}

/// A frame inline, or its encoding in contiguous fragments of one assembly.
pub(super) enum Encoded {
    Inline(proto::TerminalFrame, Option<ByteReservation>),
    Fragments(Fragments),
}
/// One shared encoding and one fragment at a time. No second vector of every
/// fragment is kept while a slow client consumes the first one.
pub(super) struct Fragments {
    bytes: Vec<u8>,
    frame_id: String,
    sha256: Vec<u8>,
    offset: usize,
    _reservation: Option<crate::remote::sync::ByteReservation>,
}
impl Iterator for Fragments {
    type Item = proto::TerminalFrameFragment;
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        let end = (self.offset + FRAGMENT_BYTES).min(self.bytes.len());
        let part = proto::TerminalFrameFragment {
            frame_id: self.frame_id.clone(),
            offset: self.offset as u64,
            total_bytes: self.bytes.len() as u64,
            sha256: self.sha256.clone(),
            data: self.bytes[self.offset..end].to_vec(),
        };
        self.offset = end;
        Some(part)
    }
}
#[cfg(test)]
pub(super) fn encode(frame: proto::TerminalFrame) -> Result<Encoded, tonic::Status> {
    encode_reserved(frame, None)
}
fn encode_reserved(
    frame: proto::TerminalFrame,
    mut reservation: Option<crate::remote::sync::ByteReservation>,
) -> Result<Encoded, tonic::Status> {
    let length = frame.encoded_len();
    if length <= INLINE_FRAME_BYTES {
        if let Some(reservation) = &mut reservation {
            reservation.shrink_to(cells_bytes(&frame.cells) + 128);
        }
        return Ok(Encoded::Inline(frame, reservation));
    }
    if length > MAX_FRAME_BYTES {
        return Err(tonic::Status::resource_exhausted(
            "The terminal frame exceeds the remote frame limit.",
        ));
    }
    let bytes = frame.encode_to_vec();
    drop(frame);
    if let Some(reservation) = &mut reservation {
        reservation.shrink_to(bytes.capacity() + 128);
    }
    let sha256 = Sha256::digest(&bytes).to_vec();
    let frame_id = uuid::Uuid::new_v4().to_string();
    Ok(Encoded::Fragments(Fragments {
        bytes,
        frame_id,
        sha256,
        offset: 0,
        _reservation: reservation,
    }))
}
