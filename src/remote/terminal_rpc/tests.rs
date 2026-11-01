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
    opens: Mutex<usize>,
    info: Mutex<Option<TerminalInfo>>,
}
#[tonic::async_trait]
impl TerminalHost for FakeHost {
    async fn terminals(&self) -> Result<Vec<TerminalInfo>, Status> {
        Ok(self.info.lock().unwrap().iter().cloned().collect())
    }
    async fn terminal(&self, id: &str) -> Result<Option<TerminalInfo>, Status> {
        Ok(self
            .info
            .lock()
            .unwrap()
            .as_ref()
            .filter(|info| info.id() == id)
            .cloned())
    }
    async fn open(&self, request: OpenTerminal) -> Result<TerminalInfo, Status> {
        *self.opens.lock().unwrap() += 1;
        let info = TerminalInfo::new(request.terminal, request.root, "Test".into(), None)
            .with_workspace(request.workspace);
        *self.info.lock().unwrap() = Some(info.clone());
        Ok(info)
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
    let roots = WorkspaceRoots::default();
    roots
        .replace(std::collections::HashMap::from([(
            "workspace".into(),
            root.into(),
        )]))
        .unwrap();
    let rpc = TerminalRpc::new(auth, roots, ResourceLocks::default(), host.clone());
    (rpc, grant, host)
}
fn meta() -> proto::CommandMeta {
    proto::CommandMeta {
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}
fn project(rpc: &TerminalRpc, root: &Path) -> proto::ProjectRef {
    let mut projects = crate::state::projects::Projects::default();
    projects.open(root.into());
    let project = proto::ProjectRef {
        host_id: rpc.auth.host_id().into(),
        project_id: serde_json::to_string(&projects.current().unwrap().id).unwrap(),
        worktree_id: None,
    };
    rpc.roots.replace_projects(projects.items).unwrap();
    project
}
async fn open(
    rpc: &TerminalRpc,
    grant: &DeviceGrant,
    project: &proto::ProjectRef,
    meta: &proto::CommandMeta,
    tool: &str,
) -> Result<proto::CommandReceipt, Status> {
    rpc.command(
        grant.clone(),
        Some(meta.clone()),
        Some(project.clone()),
        TerminalCommand::Open {
            tool: tool.into(),
            profile: None,
        },
    )
    .await
}

#[tokio::test]
async fn retry_retains_terminal_authority_after_project_removal_before_result_or_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::READ | host::TERMINAL).await;
    let meta = meta();
    let project = project(&rpc, &root);
    let first = open(&rpc, &grant, &project, &meta, "shell").await.unwrap();
    rpc.roots.replace_projects(vec![]).unwrap();
    let retry = open(&rpc, &grant, &project, &meta, "shell").await.unwrap();
    assert_eq!(first.state, proto::OperationState::Succeeded as i32);
    assert_eq!(retry.state, first.state);
    assert_eq!(retry, first);
    assert_eq!(*host.opens.lock().unwrap(), 1);
    assert_eq!(
        open(&rpc, &grant, &project, &meta, "other")
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    rpc.auth
        .client()
        .update_runtime_device_authorization(
            grant.id().into(),
            grant.authorization_revision(),
            host::READ | host::TERMINAL,
            vec![],
        )
        .await
        .unwrap();
    for tool in ["shell", "other"] {
        assert_eq!(
            open(&rpc, &grant, &project, &meta, tool)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
    }
    assert_eq!(*host.opens.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_read_only_device_cannot_run_terminal_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let meta = meta();
    let project = project(&rpc, &root);
    assert!(open(&rpc, &grant, &project, &meta, "shell").await.is_err());
    assert_eq!(*host.stops.lock().unwrap(), 0);
    assert_eq!(*host.opens.lock().unwrap(), 0);
}

#[tokio::test]
async fn private_grant_and_matching_cwd_cannot_expose_a_scratch_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::ALL_SCOPES).await;
    rpc.auth
        .client()
        .update_runtime_device_authorization(
            grant.id().into(),
            grant.authorization_revision(),
            host::ALL_SCOPES,
            vec!["*".into()],
        )
        .await
        .unwrap();
    let grant = rpc
        .auth
        .client()
        .lookup_runtime_device(vec![9; 32])
        .await
        .unwrap();
    let project = project(&rpc, &root);
    let terminal = uuid::Uuid::new_v4().to_string();
    let info = TerminalInfo::new(terminal.clone(), root.clone(), "Scratch".into(), None);
    let target = rpc.reference(&terminal);
    for scratch in [info.clone(), info.clone().with_workspace("scratch".into())] {
        *host.info.lock().unwrap() = Some(scratch);
        assert!(rpc.summaries(&grant).await.unwrap().is_empty());
        for scope in [READ, TERMINAL] {
            assert_eq!(
                rpc.terminal(&grant, Some(&target), scope)
                    .await
                    .err()
                    .unwrap()
                    .code(),
                tonic::Code::NotFound
            );
        }
        let meta = meta();
        let stop = TerminalCommand::Stop {
            terminal: terminal.clone(),
            run: uuid::Uuid::new_v4().to_string(),
        };
        let rejected = rpc
            .command(
                grant.clone(),
                Some(meta.clone()),
                Some(project.clone()),
                stop.clone(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.state, proto::OperationState::Rejected as i32);
        assert_eq!(
            rpc.command(grant.clone(), Some(meta), Some(project.clone()), stop)
                .await
                .unwrap(),
            rejected
        );
    }
    assert_eq!(*host.stops.lock().unwrap(), 0);
    let info = info.with_workspace(rpc.roots.workspace_id(&root).unwrap());
    *host.info.lock().unwrap() = Some(info);
    assert_eq!(rpc.summaries(&grant).await.unwrap().len(), 1);
    assert!(rpc.terminal(&grant, Some(&target), READ).await.is_ok());
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_rejects_unstarted_terminal_commands_and_never_replays_unknown_effects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let database = dir.path().join("db");
    let (old, grant, host) = rpc(&root, &database, host::READ | host::TERMINAL).await;
    let project = project(&old, &root);
    let target = proto::ResourceTarget {
        resource: Some(proto::resource_target::Resource::Project(project.clone())),
    };
    let command = TerminalCommand::Open {
        tool: "shell".into(),
        profile: None,
    };
    let accepted_meta = meta();
    let unknown_meta = meta();
    let removed_meta = meta();
    for (meta, attempting) in [
        (&accepted_meta, false),
        (&unknown_meta, true),
        (&removed_meta, false),
    ] {
        let prepared = api::resources::prepare(
            &old.auth,
            &old.roots,
            &grant,
            meta.clone(),
            &target,
            Command::Terminal(command.clone()),
        )
        .await
        .unwrap();
        let accepted = old
            .auth
            .client()
            .accept_runtime_command(prepared.runtime_input().clone(), host::TERMINAL)
            .await
            .unwrap();
        if attempting {
            old.auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .unwrap();
        }
    }
    let roots = old.roots.clone();
    old.auth.client().shutdown().await.unwrap();
    let client = SettingsClient::open(&database, crate::settings::Access::ReadWrite)
        .await
        .unwrap();
    let identity = client.initialize_runtime().await.unwrap();
    let grant = client.lookup_runtime_device(vec![9; 32]).await.unwrap();
    let retry = TerminalRpc::new(
        RpcAuth::new(identity.id().into(), client.clone()).unwrap(),
        roots,
        ResourceLocks::default(),
        host.clone(),
    );
    let done = open(&retry, &grant, &project, &accepted_meta, "shell")
        .await
        .unwrap();
    assert_eq!(done.state, proto::OperationState::Rejected as i32);
    assert_eq!(done.error.as_ref().unwrap().code, "conditions_changed");
    assert_eq!(
        open(&retry, &grant, &project, &accepted_meta, "shell")
            .await
            .unwrap(),
        done
    );
    assert_eq!(
        open(&retry, &grant, &project, &unknown_meta, "shell")
            .await
            .unwrap()
            .state,
        proto::OperationState::DeliveryUnknown as i32
    );
    retry.roots.replace_projects(vec![]).unwrap();
    let rejected = open(&retry, &grant, &project, &removed_meta, "shell")
        .await
        .unwrap();
    assert_eq!(rejected.state, proto::OperationState::Rejected as i32);
    assert_eq!(rejected.error.as_ref().unwrap().code, "conditions_changed");
    assert_eq!(
        open(&retry, &grant, &project, &removed_meta, "shell")
            .await
            .unwrap(),
        rejected
    );
    assert_eq!(*host.opens.lock().unwrap(), 0);
    client.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_retried_lease_never_takes_input_back_and_a_refused_takeover_is_durable() {
    use crate::terminal::{
        environment::ShellEnvironment,
        session::{LaunchSpec, Session},
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, host) = rpc(&root, &dir.path().join("db"), host::READ | host::TERMINAL).await;
    let project = project(&rpc, &root);
    let session = Arc::new(
        Session::start(
            LaunchSpec {
                program: "/bin/cat".into(),
                arguments: vec![],
                cwd: root.clone(),
            },
            &ShellEnvironment {
                shell: "/bin/sh".into(),
                vars: std::collections::HashMap::from([
                    ("PATH".into(), "/usr/bin:/bin".into()),
                    ("TERM".into(), "xterm-256color".into()),
                ]),
            },
            Size::bounded(80, 24),
        )
        .unwrap(),
    );
    let run = Run::new(session.clone(), Some(Holder::Local));
    let terminal = uuid::Uuid::new_v4().to_string();
    *host.info.lock().unwrap() = Some(
        TerminalInfo::new(
            terminal.clone(),
            root.clone(),
            "Test".into(),
            Some(run.clone()),
        )
        .with_workspace(rpc.roots.workspace_id(&root).unwrap()),
    );
    let acquire = TerminalCommand::Acquire {
        terminal: terminal.clone(),
        take_over: true,
    };
    let acquire_meta = meta();
    let first = rpc
        .command(
            grant.clone(),
            Some(acquire_meta.clone()),
            Some(project.clone()),
            acquire.clone(),
        )
        .await
        .unwrap();
    let Some(proto::command_receipt::Result::Terminal(result)) = first.result.as_ref() else {
        panic!("missing lease result")
    };
    let lease = result.lease.as_ref().unwrap();
    assert!(lease.held);
    let released = rpc
        .command(
            grant.clone(),
            Some(meta()),
            Some(project.clone()),
            TerminalCommand::Release {
                terminal: terminal.clone(),
                run: run.id().into(),
                fencing_token: lease.fencing_token,
            },
        )
        .await
        .unwrap();
    assert_eq!(released.state, proto::OperationState::Succeeded as i32);
    run.acquire(Holder::Local, true).unwrap();
    assert_eq!(
        rpc.command(
            grant.clone(),
            Some(acquire_meta.clone()),
            Some(project.clone()),
            acquire
        )
        .await
        .unwrap(),
        first
    );
    assert!(run.lease().held_by(&Holder::Local));
    assert_eq!(
        run.input(
            &Holder::device(grant.id(), grant.name()),
            lease.fencing_token,
            1,
            b"refused\n".to_vec()
        ),
        Err(Refused::StaleToken)
    );
    let refused_meta = meta();
    let refused = TerminalCommand::Acquire {
        terminal: terminal.clone(),
        take_over: false,
    };
    let rejection = rpc
        .command(
            grant.clone(),
            Some(refused_meta.clone()),
            Some(project.clone()),
            refused.clone(),
        )
        .await
        .unwrap();
    assert_eq!(rejection.state, proto::OperationState::Rejected as i32);
    assert_eq!(rejection.error.as_ref().unwrap().code, "conditions_changed");
    run.release(&Holder::Local, run.lease().token()).unwrap();
    assert_eq!(
        rpc.command(
            grant.clone(),
            Some(refused_meta.clone()),
            Some(project.clone()),
            refused
        )
        .await
        .unwrap(),
        rejection
    );
    assert!(run.lease().holder().is_none());
    assert_eq!(
        rpc.command(
            grant,
            Some(refused_meta),
            Some(project),
            TerminalCommand::Acquire {
                terminal,
                take_over: true
            }
        )
        .await
        .unwrap_err()
        .code(),
        tonic::Code::Aborted
    );
    session.stop();
    session.wait_closed().await.unwrap();
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
        TerminalInfo::new("history".into(), root.clone(), "Test".into(), None)
            .with_workspace("workspace".into()),
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
async fn removing_the_project_discards_an_unread_terminal_history_page() {
    use futures_lite::StreamExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (rpc, grant, _) = rpc(&root, &dir.path().join("db"), host::READ).await;
    let info = TerminalInfo::new("history".into(), root, "Test".into(), None)
        .with_workspace("workspace".into());
    let reservation = rpc.output.reserve_assembly(32 * 1024 * 1024).unwrap();
    let events = (0..4).map(move |_| {
        let _keep_workspace = &reservation;
        (proto::TerminalHistoryEvent::default(), None)
    });
    let mut stream = history::stream(
        rpc.clone(),
        grant.id().into(),
        info,
        Box::new(events),
        rpc.admit_stream().unwrap(),
    );
    stream.next().await.unwrap().unwrap();
    rpc.roots.replace_projects(vec![]).unwrap();
    // Wait for the blocked producer's authorization check, without polling a
    // previously queued event before that periodic check has run.
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
    assert_eq!(error.code(), tonic::Code::NotFound);
    assert!(stream.next().await.is_none());
    assert_eq!(rpc.output.queued_bytes(), 0);
    assert!(rpc.output.reserve_assembly(32 * 1024 * 1024).is_ok());
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
        TerminalInfo::new("history".into(), root.clone(), "Test".into(), None)
            .with_workspace("workspace".into()),
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
        TerminalInfo::new("history".into(), root.clone(), "Test".into(), None)
            .with_workspace("workspace".into()),
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
