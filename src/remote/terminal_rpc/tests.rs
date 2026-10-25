use super::*;
use crate::{
    host::{self, DeviceRegistration},
    settings::SettingsClient,
    terminal::session::{Cell, Frame, Mode, Size},
};
use alacritty_terminal::{
    term::cell::Flags,
    vte::ansi::{CursorShape, Rgb},
};
use prost::Message;
use std::sync::Mutex;

fn cell(row: usize, column: usize, text: &str, width: usize, flags: Flags) -> Cell {
    Cell {
        row,
        column,
        text: text.into(),
        width,
        fg: Rgb { r: 1, g: 2, b: 3 },
        bg: Rgb {
            r: 250,
            g: 251,
            b: 252,
        },
        flags,
        selected: false,
    }
}
fn frame(columns: usize, rows: usize, cells: Vec<Cell>) -> Frame {
    Frame {
        cells,
        cursor: (0, 1),
        cursor_shape: CursorShape::Block,
        mode: Mode::SHOW_CURSOR | Mode::BRACKETED_PASTE | Mode::ALT_SCREEN,
        offset: 0,
        size: Size { columns, rows },
        revision: 1,
    }
}

#[test]
fn live_projection_uses_wire_masks_spans_and_patches_only_changes() {
    let mut installed = projection::Installed::default();
    let first = frame(
        3,
        1,
        vec![
            cell(0, 0, "界", 2, Flags::WIDE_CHAR | Flags::BOLD),
            cell(0, 1, " ", 1, Flags::WIDE_CHAR_SPACER),
            cell(
                0,
                2,
                "e\u{301}",
                1,
                Flags::INVERSE | Flags::DIM | Flags::UNDERLINE,
            ),
        ],
    );
    let complete = installed.next(&first, false).unwrap();
    assert!(complete.complete);
    assert_eq!(complete.cells.len(), 3);
    let wide = &complete.cells[0];
    assert_eq!((wide.text.as_str(), wide.column_span), ("界", 2));
    assert_eq!(
        wide.flags,
        proto::TerminalCellFlag::CellWide as u32 | proto::TerminalCellFlag::CellBold as u32
    );
    assert_eq!(wide.foreground_rgba, 0x010203ff);
    assert_eq!(wide.background_rgba, 0xfafbfcff);
    let spacer = &complete.cells[1];
    assert_eq!((spacer.text.as_str(), spacer.column_span), ("", 0));
    // Inverse and dim are already in the colors; only underline is an attribute.
    assert_eq!(
        complete.cells[2].flags,
        proto::TerminalCellFlag::CellUnderline as u32
    );
    assert_eq!(
        complete.input_modes,
        proto::TerminalInputMode::CursorVisible as u64
            | proto::TerminalInputMode::BracketedPaste as u64
            | proto::TerminalInputMode::AlternateScreen as u64
    );
    assert_eq!((complete.cursor_row, complete.cursor_column), (0, 1));
    assert!(installed.next(&first, false).is_none());

    let mut second = first.clone();
    second.cells[2] = cell(0, 2, "x", 1, Flags::empty());
    let patch = installed.next(&second, true).unwrap();
    assert!(!patch.complete && patch.lost_scrollback);
    assert_eq!(patch.cells.len(), 1);
    assert_eq!(patch.cells[0].column, 2);

    let resized = frame(1, 1, vec![cell(0, 0, "a", 1, Flags::empty())]);
    assert!(installed.next(&resized, true).unwrap().complete);
}

#[test]
fn oversized_frames_are_contiguous_verified_fragments() {
    let cells = (0..200)
        .flat_map(|row| (0..500).map(move |column| cell(row, column, "\u{1f600}", 1, Flags::BOLD)))
        .collect();
    let wire = projection::page(&frame(500, 200, cells), false);
    let projection::Encoded::Fragments(fragments) = projection::encode(wire.clone()).unwrap()
    else {
        panic!("a large frame must be fragmented");
    };
    let fragments: Vec<_> = fragments.collect();
    let mut assembled = Vec::new();
    for fragment in &fragments {
        assert!(fragment.data.len() <= projection::FRAGMENT_BYTES);
        assert_eq!(fragment.offset as usize, assembled.len());
        assert_eq!(fragment.frame_id, fragments[0].frame_id);
        assembled.extend_from_slice(&fragment.data);
    }
    assert_eq!(assembled.len() as u64, fragments[0].total_bytes);
    assert_eq!(
        Sha256::digest(&assembled).as_slice(),
        fragments[0].sha256.as_slice()
    );
    assert_eq!(
        proto::TerminalFrame::decode(assembled.as_slice()).unwrap(),
        wire
    );
}

#[derive(Default)]
pub(super) struct FakeHost {
    stops: Mutex<usize>,
}
#[tonic::async_trait]
impl TerminalHost for FakeHost {
    async fn terminals(&self) -> Result<Vec<TerminalInfo>, Status> {
        Ok(vec![])
    }
    async fn terminal(&self, _: &str) -> Result<Option<TerminalInfo>, Status> {
        Ok(None)
    }
    async fn open(&self, _: OpenTerminal) -> Result<TerminalInfo, Status> {
        Err(Status::unimplemented("not used"))
    }
    async fn stop(&self, _: &str, _: &str) -> Result<(), Status> {
        *self.stops.lock().unwrap() += 1;
        Ok(())
    }
    async fn restart(&self, _: &str, _: &str, _: Holder) -> Result<String, Status> {
        Err(Status::unimplemented("not used"))
    }
}
pub(super) async fn rpc(
    root: &Path,
    database: &Path,
    scopes: u32,
) -> (TerminalRpc, DeviceGrant, Arc<FakeHost>) {
    let client = SettingsClient::create(database).await.unwrap();
    let host_record = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "Phone".into(),
                vec![9; 32],
                scopes,
                vec![root.to_str().unwrap().into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![9; 32]).await.unwrap();
    let auth = RpcAuth::new(host_record.id().into(), client).unwrap();
    let host = Arc::new(FakeHost::default());
    let rpc = TerminalRpc::new(
        auth,
        WorkspaceRoots::default(),
        ResourceLocks::default(),
        host.clone(),
    );
    (rpc, grant, host)
}
fn meta(rpc: &TerminalRpc) -> proto::CommandMeta {
    proto::CommandMeta {
        host_id: rpc.auth.host_id().into(),
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}
async fn stop(
    rpc: &TerminalRpc,
    grant: &DeviceGrant,
    root: &Path,
    meta: &proto::CommandMeta,
) -> Result<proto::CommandReceipt, Status> {
    let host = rpc.host.clone();
    let input = rpc.input(
        grant,
        Some(meta),
        "terminal.stop",
        "terminal".into(),
        b"run".to_vec(),
        root,
    )?;
    rpc.command(input, move || async move {
        host.stop("terminal", "run").await?;
        Ok(Vec::new())
    })
    .await
}

#[tokio::test]
async fn a_retried_stop_returns_the_recorded_outcome_without_stopping_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::READ | host::TERMINAL).await;
    let meta = meta(&rpc);
    let first = stop(&rpc, &grant, &root, &meta).await.unwrap();
    let retry = stop(&rpc, &grant, &root, &meta).await.unwrap();
    assert_eq!(first.state, proto::OperationState::Succeeded as i32);
    assert_eq!(retry.state, first.state);
    assert_eq!(*host.stops.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_read_only_device_cannot_run_terminal_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let meta = meta(&rpc);
    assert!(stop(&rpc, &grant, &root, &meta).await.is_err());
    assert_eq!(*host.stops.lock().unwrap(), 0);
}

#[tokio::test]
async fn history_tokens_bind_run_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, _, _) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let token = history::token(&rpc, "run-a", 7, 40);
    assert_eq!(history::resolve(&rpc, "run-a", 7, &token).unwrap(), 40);
    // New output since the page: read again rather than mixing pages.
    assert_eq!(
        history::resolve(&rpc, "run-a", 8, &token)
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    // Another run or an edited offset is not this server's token.
    for (run, token) in [
        ("run-b", token.clone()),
        ("run-a", token.replacen("40", "41", 1)),
    ] {
        assert_eq!(
            history::resolve(&rpc, run, 7, &token).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }
}

#[test]
fn fragmented_projection_admission_includes_retained_baselines_and_recovers_after_drop() {
    use crate::remote::sync::{Limits, ReplayHub};
    let hub = ReplayHub::new(Limits::default());
    let make = || {
        frame(
            500,
            200,
            (0..200)
                .flat_map(|row| (0..500).map(move |col| cell(row, col, "界", 2, Flags::WIDE_CHAR)))
                .collect(),
        )
    };
    let mut installed = projection::Installed::default();
    let reservation = projection::admit(&hub, 100_000, 1_600_000).unwrap();
    let projection::Encoded::Fragments(mut fragments) = installed
        .next_bounded(make(), false, reservation)
        .unwrap()
        .unwrap()
    else {
        panic!("must fragment");
    };
    let first = fragments.next().unwrap();
    assert_eq!(first.offset, 0);
    assert_eq!(first.data.len(), projection::FRAGMENT_BYTES);
    // The first frame's baseline and lazy encoding still consume workspace.
    assert!(projection::admit(&hub, 100_000, 1_600_000).is_err());
    let mut bytes = first.data;
    for fragment in fragments.by_ref() {
        assert_eq!(fragment.offset as usize, bytes.len());
        bytes.extend_from_slice(&fragment.data);
    }
    let decoded = proto::TerminalFrame::decode(bytes.as_slice()).unwrap();
    assert!(decoded.complete);
    assert_eq!(decoded.cells.len(), 100_000);
    drop((decoded, bytes, fragments));
    // Baseline remains charged after the encoding is dropped.
    assert!(hub.reserve_assembly(32 * 1024 * 1024).is_err());
    drop(installed);
    let mut recovered = projection::Installed::default();
    let reservation = projection::admit(&hub, 100_000, 1_600_000).unwrap();
    let encoded = recovered
        .next_bounded(make(), false, reservation)
        .unwrap()
        .unwrap();
    drop((encoded, recovered));
    assert!(hub.reserve_assembly(32 * 1024 * 1024).is_ok());
}

#[tokio::test]
async fn unread_history_times_out_and_returns_workspace_before_reader_polls() {
    use futures_lite::StreamExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, _) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let reservation = rpc.output.reserve_assembly(32 * 1024 * 1024).unwrap();
    let events = Box::new((0..3).map(move |_| {
        let _keep_workspace = &reservation;
        (proto::TerminalHistoryEvent::default(), None)
    }));
    let mut stream = history::stream(
        rpc.clone(),
        grant.id().into(),
        root.to_str().unwrap().into(),
        events,
        rpc.admit_stream().unwrap(),
    );
    // A full response channel must not keep an abandoned large page alive.
    tokio::time::timeout(std::time::Duration::from_secs(7), async {
        loop {
            if rpc.output.reserve_assembly(32 * 1024 * 1024).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().code(),
        tonic::Code::ResourceExhausted
    );
    assert!(stream.next().await.is_none());
    assert_eq!(rpc.output.queued_bytes(), 0);
    assert_eq!(rpc.streams.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn history_drains_end_on_success_but_revocation_discards_an_unread_page() {
    use futures_lite::StreamExt;
    use proto::terminal_history_event::Part;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, _) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let events = [
        Part::Start(proto::TerminalHistoryStart::default()),
        Part::Frame(proto::TerminalFrame::default()),
        Part::End(proto::Cursor::default()),
    ]
    .into_iter()
    .map(|part| {
        (
            proto::TerminalHistoryEvent {
                part: Some(part),
                ..Default::default()
            },
            None,
        )
    });
    let mut stream = history::stream(
        rpc.clone(),
        grant.id().into(),
        root.to_str().unwrap().into(),
        Box::new(events),
        rpc.admit_stream().unwrap(),
    );
    assert!(matches!(
        stream.next().await.unwrap().unwrap().part,
        Some(Part::Start(_))
    ));
    assert!(matches!(
        stream.next().await.unwrap().unwrap().part,
        Some(Part::Frame(_))
    ));
    assert!(matches!(
        stream.next().await.unwrap().unwrap().part,
        Some(Part::End(_))
    ));
    assert!(stream.next().await.is_none());

    let reservation = rpc.output.reserve_assembly(32 * 1024 * 1024).unwrap();
    let events = (0..4).map(move |_| {
        let _keep_workspace = &reservation;
        (proto::TerminalHistoryEvent::default(), None)
    });
    let mut stream = history::stream(
        rpc.clone(),
        grant.id().into(),
        root.to_str().unwrap().into(),
        Box::new(events),
        rpc.admit_stream().unwrap(),
    );
    stream.next().await.unwrap().unwrap();
    rpc.auth
        .client()
        .revoke_runtime_device(grant.id().into())
        .await
        .unwrap();
    // Revoke must release the remaining page even without another transport poll.
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if rpc.output.reserve_assembly(32 * 1024 * 1024).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(
        error.code(),
        tonic::Code::PermissionDenied | tonic::Code::Unauthenticated
    ));
    assert!(stream.next().await.is_none());
    assert_eq!(rpc.output.queued_bytes(), 0);
}
