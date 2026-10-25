//! One TerminalSession stream: a subscriber's projection, its lease and its input.
//! A frame is computed only when the outbound queue has room, so a slow client
//! receives fewer, coalesced patches and never holds the parser or other views.
//! Acks mean queued on the host, never executed by the shell.
use super::{StreamSlot, TerminalInfo, TerminalRpc, projection, proto, refused};
use crate::{
    host::{DeviceGrant, READ, TERMINAL},
    remote::auth::{WorkspaceRoots, storage_status},
    terminal::control::{Attachment, Holder, RENEW_INTERVAL, Run},
};
use futures_lite::StreamExt;
use prost::Message;
use proto::terminal_server_event::Event;
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};
use tonic::{Status, Streaming};

/// Outbound events buffered for the transport.
const OUTBOUND: usize = 1;
/// Control events (acks, lease, errors) waiting for the client; beyond this the
/// client is not reading and must resubscribe.
const PENDING: usize = 64;
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(10);
/// A write re-reads the device's terminal grant at most this often.
const WRITE_RECHECK: Duration = Duration::from_secs(1);

pub(super) async fn open(
    rpc: TerminalRpc,
    grant: DeviceGrant,
    credential: Vec<u8>,
    mut inbound: Streaming<proto::TerminalClientEvent>,
    slot: StreamSlot,
) -> Result<super::RpcStream<proto::TerminalServerEvent>, Status> {
    let first = tokio::time::timeout(SUBSCRIBE_TIMEOUT, inbound.message())
        .await
        .map_err(|_| Status::deadline_exceeded("Subscribe to the terminal first."))??;
    let Some(proto::TerminalClientEvent {
        event: Some(proto::terminal_client_event::Event::Subscribe(subscribe)),
    }) = first
    else {
        return Err(Status::invalid_argument(
            "The first terminal event must subscribe.",
        ));
    };
    let (grant, info) = rpc
        .terminal(&grant, subscribe.terminal.as_ref(), READ)
        .await?;
    let run = info
        .run()
        .cloned()
        .ok_or_else(|| Status::unavailable("The terminal is starting. Try again shortly."))?;
    let (sender, receiver) = mpsc::channel(OUTBOUND);
    let view = View {
        _attachment: run.attach(grant.id()),
        stream: format!("terminal/{}/{}", info.id(), uuid::Uuid::new_v4()),
        rpc,
        grant,
        credential,
        info,
        run,
        sequence: 0,
        installed: projection::Installed::default(),
        fragments: None,
        material: None,
        wire: None,
        pending: VecDeque::new(),
        frame_due: true,
        lease_due: true,
        write_checked: None,
        end: None,
    };
    let (finished, completion) = oneshot::channel();
    tokio::spawn(async move {
        let reason = view.serve(inbound, sender, slot).await;
        let _ = finished.send(reason);
    });
    // Completion bypasses a full queue: a nonreader gets an explicit resync
    // reason on its next poll, never a successful EOF with a partial assembly.
    Ok(Box::pin(futures_lite::stream::unfold(
        (receiver, Some(completion)),
        |(mut receiver, completion)| async move {
            let mut completion = completion?;
            tokio::select! {
                biased;
                reason = &mut completion => {
                    receiver.close();
                    while receiver.try_recv().is_ok() {}
                    Some((Err(reason.unwrap_or_else(|_| Status::unavailable("Terminal stream ended. Resubscribe."))), (receiver, None)))
                }
                event = receiver.recv() => {
                    match event {
                        Some((event, _wire, _material)) => Some((Ok(event), (receiver, Some(completion)))),
                        None => Some((Err(completion.await.unwrap_or_else(|_| Status::unavailable("Terminal stream ended. Resubscribe."))), (receiver, None))),
                    }
                }
            }
        },
    )))
}

struct View {
    rpc: TerminalRpc,
    grant: DeviceGrant,
    credential: Vec<u8>,
    info: TerminalInfo,
    run: Arc<Run>,
    _attachment: Attachment,
    stream: String,
    sequence: u64,
    installed: projection::Installed,
    fragments: Option<projection::Fragments>,
    material: Option<crate::remote::sync::ByteReservation>,
    pending: VecDeque<(Event, crate::remote::sync::ByteReservation)>,
    wire: Option<crate::remote::sync::ByteReservation>,
    frame_due: bool,
    lease_due: bool,
    write_checked: Option<std::time::Instant>,
    /// Set when the stream must end after flushing pending events.
    end: Option<Status>,
}
impl View {
    fn holder(&self) -> Holder {
        Holder::device(self.grant.id(), self.grant.name())
    }
    fn event(&mut self, event: Event) -> proto::TerminalServerEvent {
        self.sequence += 1;
        proto::TerminalServerEvent {
            terminal: Some(self.rpc.reference(self.info.id())),
            run_id: self.run.id().into(),
            cursor: Some(proto::Cursor {
                runtime_epoch: self.rpc.epoch.to_string(),
                stream_id: self.stream.clone(),
                sequence: self.sequence,
            }),
            event: Some(event),
        }
    }
    fn push(&mut self, event: Event) {
        if self.pending.len() >= PENDING {
            self.end = Some(Status::resource_exhausted(
                "The terminal view is not reading events. Subscribe again.",
            ));
            return;
        }
        match self
            .rpc
            .output
            .reserve_output(self.grant.id(), event.encoded_len() + 1024)
        {
            Ok(charge) => self.pending.push_back((event, charge)),
            Err(_) => {
                self.end = Some(Status::resource_exhausted(
                    "Terminal control output budget is full. Resubscribe.",
                ))
            }
        }
    }
    fn error(&mut self, error: &Status) {
        self.push(Event::Error(super::domain_error(error)));
    }
    /// The verified credential and its grant for this root, re-read periodically
    /// so a revoked device or root stops receiving terminal bytes.
    async fn reauthorize(&mut self, scope: u32) -> Result<(), Status> {
        let grant = self
            .rpc
            .auth
            .client()
            .lookup_runtime_device(self.credential.clone())
            .await
            .map_err(storage_status)?;
        if grant.id() != self.grant.id() {
            return Err(Status::permission_denied("The device identity changed."));
        }
        // Keeping READ after losing TERMINAL must not keep the old input lease.
        // This also runs on host authorization pulses while output is blocked.
        if self.rpc.auth.scope(&grant, TERMINAL).is_err() {
            let lease = self.run.lease();
            if lease.held_by(&self.holder()) {
                let _ = self.run.release(&self.holder(), lease.token());
            }
        }
        self.rpc.auth.scope(&grant, scope)?;
        self.grant = self
            .rpc
            .auth
            .resource(
                grant.id(),
                scope,
                WorkspaceRoots::root_string(self.info.root())?,
            )
            .await?;
        Ok(())
    }
    fn check_target(&self, terminal: Option<&proto::TerminalRef>, run: &str) -> Result<(), Status> {
        let same = terminal.is_some_and(|t| {
            t.host_id == self.rpc.auth.host_id() && t.terminal_id == self.info.id()
        });
        if !same {
            return Err(Status::invalid_argument(
                "This stream serves a different terminal.",
            ));
        }
        if run != self.run.id() {
            return Err(Status::aborted(
                "The terminal run changed. Refresh the terminal.",
            ));
        }
        Ok(())
    }
    async fn client(&mut self, event: proto::terminal_client_event::Event) {
        use proto::terminal_client_event::Event as Client;
        match event {
            Client::Subscribe(_) => self.error(&Status::invalid_argument(
                "This stream is already subscribed.",
            )),
            Client::Input(input) => {
                let outcome = match self.writable(input.terminal.as_ref(), &input.run_id).await {
                    Ok(()) => self
                        .run
                        .input(
                            &self.holder(),
                            input.fencing_token,
                            input.input_id,
                            input.data,
                        )
                        .map_err(|error| refused(&error)),
                    Err(error) => Err(error),
                };
                self.push(Event::InputAck(proto::TerminalInputAck {
                    input_id: input.input_id,
                    queued: outcome.is_ok(),
                    error: outcome.err().map(|error| super::domain_error(&error)),
                }));
            }
            Client::Resize(resize) => {
                let outcome = match self
                    .writable(resize.terminal.as_ref(), &resize.run_id)
                    .await
                {
                    Ok(()) => self
                        .run
                        .resize(
                            &self.holder(),
                            resize.fencing_token,
                            crate::terminal::session::Size {
                                columns: resize.columns as usize,
                                rows: resize.rows as usize,
                            },
                        )
                        .map_err(|error| refused(&error)),
                    Err(error) => Err(error),
                };
                if let Err(error) = outcome {
                    self.error(&error);
                }
            }
            Client::Renew(renew) => {
                let outcome = match self.writable(renew.terminal.as_ref(), &renew.run_id).await {
                    Ok(()) => self
                        .run
                        .renew(&self.holder(), renew.fencing_token)
                        .map_err(|error| refused(&error)),
                    Err(error) => Err(error),
                };
                if let Err(error) = outcome {
                    self.error(&error);
                }
                self.lease_due = true;
            }
        }
    }
    /// Writing needs the terminal scope now, not only when the stream opened.
    async fn writable(
        &mut self,
        terminal: Option<&proto::TerminalRef>,
        run: &str,
    ) -> Result<(), Status> {
        self.check_target(terminal, run)?;
        if self
            .write_checked
            .is_some_and(|at| at.elapsed() < WRITE_RECHECK)
        {
            return Ok(());
        }
        self.write_checked = None;
        self.reauthorize(TERMINAL).await?;
        self.write_checked = Some(std::time::Instant::now());
        Ok(())
    }
    /// The next event to send, computing the projection only now.
    fn next(&mut self) -> Option<proto::TerminalServerEvent> {
        if let Some(fragments) = &mut self.fragments {
            if let Some(fragment) = fragments.next() {
                return Some(self.event(Event::FrameFragment(fragment)));
            }
            self.fragments = None;
        }
        if let Some((event, charge)) = self.pending.pop_front() {
            self.wire = Some(charge);
            return Some(self.event(event));
        }
        if self.frame_due {
            let mut projected = self
                .run
                .session()
                .live_frame_bounded(|cells, text| projection::admit(&self.rpc.output, cells, text));
            if projected.is_err() {
                // A large replacement may fit alone but not beside the previous
                // baseline. Release it and send a complete frame, never a patch
                // against a baseline we no longer retain.
                self.installed = projection::Installed::default();
                projected = self.run.session().live_frame_bounded(|cells, text| {
                    projection::admit(&self.rpc.output, cells, text)
                });
            }
            match projected {
                Ok(Some(((frame, lost), reservation))) => {
                    self.frame_due = false;
                    match self.installed.next_bounded(frame, lost, reservation) {
                        Ok(Some(projection::Encoded::Inline(frame, material))) => {
                            self.material = material;
                            return Some(self.event(Event::Frame(frame)));
                        }
                        Ok(Some(projection::Encoded::Fragments(fragments))) => {
                            self.fragments = Some(fragments);
                            return self.next();
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.end = Some(error);
                            return None;
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.end = Some(error);
                    return None;
                }
            }
        }
        if self.lease_due {
            self.lease_due = false;
            let lease = self.rpc.lease(self.info.id(), &self.run, self.grant.id());
            return Some(self.event(Event::Lease(lease)));
        }
        None
    }
    async fn serve(
        mut self,
        mut inbound: impl futures_lite::Stream<Item = Result<proto::TerminalClientEvent, Status>>
        + Unpin,
        sender: mpsc::Sender<(
            proto::TerminalServerEvent,
            crate::remote::sync::ByteReservation,
            Option<crate::remote::sync::ByteReservation>,
        )>,
        _slot: StreamSlot,
    ) -> Status {
        let mut output = self.run.session().observe();
        let mut lease = self.run.observe();
        let mut tick = tokio::time::interval(RENEW_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut host = self.rpc.state.as_ref().map(|state| state.watch());
        let mut stalled = None;
        let mut retry_at = tokio::time::Instant::now();
        loop {
            if let Some(error) = self.end.take() {
                return error;
            }
            let has_work = self.fragments.is_some()
                || !self.pending.is_empty()
                || self.frame_due
                || self.lease_due;
            let deadline = if has_work {
                *stalled.get_or_insert_with(|| tokio::time::Instant::now() + Duration::from_secs(5))
            } else {
                stalled = None;
                tokio::time::Instant::now() + Duration::from_secs(5)
            };
            // One output per iteration. Input/renew, revoke and run lifecycle
            // remain polled even while the outbound queue is full or fragmenting.
            tokio::select! {
                _ = host_changed(&mut host) => {
                    if let Some(state) = &self.rpc.state
                        && let Err(error) = state.accepting() { return error; }
                    self.write_checked = None;
                    if let Err(error) = self.reauthorize(READ).await { return error; }
                }
                message = inbound.next() => match message {
                    Some(Ok(proto::TerminalClientEvent { event: Some(event) })) => {
                        self.client(event).await;
                    }
                    Some(Ok(_)) => self.error(&Status::invalid_argument("Empty terminal event.")),
                    // Dropping the attachment releases this device's lease;
                    // the host continues owning the running PTY.
                    None | Some(Err(_)) => return Status::cancelled("The terminal client disconnected."),
                },
                changed = output.changed() => {
                    if changed.is_err() { return Status::unavailable("Terminal output ended. Resubscribe."); }
                    self.frame_due = true;
                }
                changed = lease.changed() => {
                    if changed.is_err() { return Status::unavailable("Terminal lease ended. Resubscribe."); }
                    if self.run.superseded() { return Status::aborted("The terminal restarted. Resubscribe."); }
                    self.lease_due = true;
                }
                _ = tick.tick() => {
                    self.run.expire();
                    self.write_checked = None;
                    if let Err(error) = self.reauthorize(READ).await { return error; }
                }
                _ = tokio::time::sleep_until(deadline), if has_work => {
                    return Status::resource_exhausted("The terminal view is not reading events. Resubscribe for a fresh snapshot.");
                }
                permit = async {
                    tokio::time::sleep_until(retry_at).await;
                    sender.reserve().await
                }, if has_work => {
                    let Ok(permit) = permit else { return Status::cancelled("The terminal client disconnected."); };
                    if let Some(event) = self.next() {
                        let charge = match self.wire.take().map(Ok).unwrap_or_else(|| self.rpc.output.reserve_output(self.grant.id(), event.encoded_len())) {
                            Ok(charge) => charge,
                            Err(_) => return Status::resource_exhausted("Terminal output budget is full. Resubscribe for a fresh snapshot."),
                        };
                        permit.send((event, charge, self.material.take()));
                        stalled = None;
                    } else {
                        // The parser briefly owns its grid, or the projection
                        // was unchanged. Do not spin on an available queue slot.
                        retry_at = tokio::time::Instant::now() + Duration::from_millis(16);
                    }
                }
                _ = sender.closed() => return Status::cancelled("The terminal client disconnected."),
            }
        }
    }
}
async fn host_changed(host: &mut Option<tokio::sync::watch::Receiver<proto::HostStatus>>) {
    if let Some(host) = host {
        let _ = host.changed().await;
    } else {
        std::future::pending::<()>().await;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::terminal::{
        environment::ShellEnvironment,
        session::{LaunchSpec, Session, Size},
    };
    use std::collections::HashMap;

    #[tokio::test]
    async fn nonreading_output_does_not_starve_input_resize_or_confirmed_disconnect() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (rpc, grant, _) =
            super::super::tests::rpc(&root, &dir.path().join("db"), READ | TERMINAL).await;
        // A controlled child only; no user shell, CLI profile, model or service.
        let session = Arc::new(
            Session::start(
                LaunchSpec {
                    program: "/bin/cat".into(),
                    arguments: vec![],
                    cwd: root.clone(),
                },
                &ShellEnvironment {
                    shell: "/bin/sh".into(),
                    vars: HashMap::from([
                        ("PATH".into(), "/usr/bin:/bin".into()),
                        ("TERM".into(), "xterm-256color".into()),
                    ]),
                },
                Size::bounded(80, 24),
            )
            .unwrap(),
        );
        let run = Run::new(
            session.clone(),
            Some(Holder::device(grant.id(), grant.name())),
        );
        let info = TerminalInfo::new(
            "test-terminal".into(),
            root,
            "Test".into(),
            Some(run.clone()),
        );
        let target = Some(rpc.reference(info.id()));
        let state = crate::remote::host::HostState::new(rpc.auth.host_id(), vec![]);
        state.set(proto::HostReadiness::Ready, String::new());
        let rpc = rpc.with_state(state.clone());
        let auth = rpc.auth.clone();
        let device = grant.clone();
        let slot = rpc.admit_stream().unwrap();
        let view = View {
            _attachment: run.attach(grant.id()),
            stream: "test-stream".into(),
            rpc,
            grant,
            credential: vec![9; 32],
            info,
            run: run.clone(),
            sequence: 0,
            installed: projection::Installed::default(),
            fragments: None,
            material: None,
            wire: None,
            pending: VecDeque::new(),
            frame_due: true,
            lease_due: true,
            write_checked: None,
            end: None,
        };
        let (input, inbound) = mpsc::channel(4);
        let (output, receiver) = mpsc::channel(OUTBOUND);
        let full = output.clone();
        let task = tokio::spawn(view.serve(
            tokio_stream::wrappers::ReceiverStream::new(inbound),
            output,
            slot,
        ));
        tokio::time::timeout(Duration::from_secs(2), async {
            while full.capacity() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Do not poll receiver. The old send-wait loop would now ignore input
        // until its five-second timeout and would not notice EOF promptly.
        input
            .send(Ok(proto::TerminalClientEvent {
                event: Some(proto::terminal_client_event::Event::Input(
                    proto::TerminalInput {
                        terminal: target.clone(),
                        run_id: run.id().into(),
                        fencing_token: run.lease().token(),
                        input_id: 1,
                        data: b"proof\n".to_vec(),
                    },
                )),
            }))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if session.live_frame().is_some_and(|(frame, _)| {
                    frame
                        .cells
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<String>()
                        .contains("proof")
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        input
            .send(Ok(proto::TerminalClientEvent {
                event: Some(proto::terminal_client_event::Event::Resize(
                    proto::TerminalResize {
                        terminal: target,
                        run_id: run.id().into(),
                        fencing_token: run.lease().token(),
                        columns: 96,
                        rows: 24,
                    },
                )),
            }))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while session.size().columns != 96 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let read_only = auth
            .client()
            .update_runtime_device_authorization(
                device.id().into(),
                device.authorization_revision(),
                READ,
                vec![dir.path().canonicalize().unwrap().to_str().unwrap().into()],
            )
            .await
            .unwrap();
        state.authorization_changed();
        tokio::time::timeout(Duration::from_secs(1), async {
            while run.lease().holder().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            !task.is_finished(),
            "READ can continue after input permission is removed"
        );
        auth.client()
            .update_runtime_device_authorization(
                device.id().into(),
                read_only.authorization_revision(),
                READ | TERMINAL,
                vec![dir.path().canonicalize().unwrap().to_str().unwrap().into()],
            )
            .await
            .unwrap();
        state.authorization_changed();
        run.acquire(Holder::device(device.id(), device.name()), false)
            .unwrap();
        drop(input);
        let reason = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reason.code(), tonic::Code::Cancelled);
        assert!(run.lease().holder().is_none());
        assert!(matches!(
            session.status(),
            crate::terminal::session::Status::Running
        ));
        drop((receiver, full));
        session.stop();
        session.wait_closed().await.unwrap();
    }
}
