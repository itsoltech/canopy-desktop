//! The shared chat projection against a fake app bridge (the test plays the runtime)
//! and real storage, replay hub and artifact store: snapshot/tail boundary, replay
//! after reconnect, observer counts, grace cleanup, save proofs and bounded transfers.
use super::*;
use crate::app_state::{
    chat_bridge::{
        ChatEvent,
        fake::{self, Call, FakeApp},
    },
    chats::ChatSnapshot,
};
use canopy_desktop::{
    chat::{agent::Capabilities, config::ChatSettings, history, transcript as t},
    command_bus::{CommandBus, Conditions, Principal},
    host::DeviceRegistration,
    remote::sync::{Limits, ReplayHub},
    settings::{ChatHistoryBatch, SettingsClient},
    state::workspace::{PaneKind, PaneMetadata},
};
use futures_lite::StreamExt;
use std::time::Duration;

struct Harness {
    rpc: ChatsRpc,
    app: FakeApp,
    record: ChatRecord,
    grant: DeviceGrant,
    client: SettingsClient,
    replay: ReplayHub,
    _dir: tempfile::TempDir,
}

async fn harness(grace: Duration, limits: Limits) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&dir.path().join("settings.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "phone".into(),
                vec![9; 32],
                host::READ | host::CHAT,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![9; 32]).await.unwrap();
    let chat = ChatSettings::new_id();
    let record = ChatRecord::new(
        chat.clone(),
        None,
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            cwd: Some(dir.path().to_owned()),
            profile_id: Some("claude-default".into()),
            chat: Some(Box::new(ChatSettings {
                id: Some(chat),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let record = client.save_runtime_chat(record).await.unwrap();
    let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
    let artifacts = Arc::new(
        canopy_desktop::artifacts::ArtifactStore::open(client.clone(), dir.path().join("a"))
            .unwrap(),
    );
    let state = HostState::new(host.id(), vec![]);
    state.set(
        canopy_desktop::api::proto::HostReadiness::Ready,
        String::new(),
    );
    let replay = ReplayHub::new(limits);
    let (bridge, app) = fake::bridge();
    let mut rpc = ChatsRpc::new(
        auth,
        bridge,
        artifacts,
        state,
        dir.path().join("chat-attachments"),
        replay.clone(),
    );
    rpc.grace = grace;
    Harness {
        rpc,
        app,
        record,
        grant,
        client,
        replay,
        _dir: dir,
    }
}

fn item(id: u64, body: t::Body) -> t::Item {
    t::Item {
        id: t::ItemId(id),
        turn: t::TurnId(1),
        parent: None,
        body,
        trimmed: false,
    }
}
fn text(id: u64, text: &str) -> t::Item {
    item(
        id,
        t::Body::Text(t::TextBlock {
            text: text.into(),
            streaming: true,
        }),
    )
}
fn turn_end(id: u64) -> t::Item {
    item(
        id,
        t::Body::TurnEnd(t::TurnEnd {
            outcome: t::Outcome::Completed,
            usage: None,
            cost_usd: None,
            duration_ms: None,
            resume_at: None,
            quiet: false,
        }),
    )
}
fn snapshot(chat: &str) -> ChatSnapshot {
    ChatSnapshot {
        chat: chat.into(),
        tool: "claude".into(),
        agent: "Claude".into(),
        profile: "Default".into(),
        title: None,
        run: 7,
        serial: 1,
        history_epoch: 0,
        status: "Ready",
        working: false,
        error: None,
        notice: None,
        recovery: None,
        account_locked: false,
        capabilities: Capabilities::default(),
        options: vec![],
        decisions: vec![],
        queue: None,
        observers: 0,
        goal: None,
        edit_unavailable: None,
        shell_unavailable: None,
    }
}

type Events = RpcStream<proto::ThreadEvent>;

#[tokio::test]
async fn typed_fragment_conversion_stops_disclosure_after_revocation() {
    let h = harness(Duration::from_secs(5), Limits::default()).await;
    let record = h.client.save_runtime_chat(h.record.clone()).await.unwrap();
    let thread = reference(&h.rpc.auth, &record);
    let item = proto::TranscriptItem {
        key: Some(proto::ItemKey {
            history_epoch: 0,
            item_id: 1,
        }),
        body: Some(proto::transcript_item::Body::Text(proto::TextBlock {
            text: "x".repeat(128 * 1024),
            streaming: false,
        })),
        ..Default::default()
    };
    let events = canopy_desktop::remote::transcript::fragments(&item)
        .unwrap()
        .into_iter()
        .map(|fragment| {
            Ok(proto::ThreadEvent {
                thread: thread.clone(),
                event: Some(proto::thread_event::Event::ItemFragment(fragment)),
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    let converted = Box::pin(futures_lite::stream::iter(events));
    let mut stream = super::api::authorized(h.rpc.clone(), record, h.grant.clone(), converted);
    assert!(stream.next().await.unwrap().is_ok());
    h.client
        .revoke_runtime_device(h.grant.id().into())
        .await
        .unwrap();
    // Each buffered fragment rechecks the current grant before disclosure.
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().code(),
        tonic::Code::PermissionDenied
    );
}

/// Subscribes a client; the first one creates the projection, for which the test
/// answers as the runtime with `initial` items. Returns the stream, and the
/// runtime's event sender when a projection was created.
async fn subscribe(
    h: &Harness,
    since: Option<proto::Cursor>,
    initial: Option<Vec<t::Item>>,
) -> (
    Result<Events, Status>,
    Option<(u64, async_channel::Sender<ChatEvent>)>,
) {
    let task = tokio::spawn({
        let (rpc, record, grant) = (h.rpc.clone(), h.record.clone(), h.grant.clone());
        async move { rpc.subscribe(record, grant, since).await }
    });
    let mut runtime = None;
    if let Some(items) = initial {
        let (id, chat, events) =
            tokio::time::timeout(Duration::from_secs(5), h.app.subscribed.recv())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(chat, h.record.id());
        let epochs = vec![crate::app_state::chats::SnapshotEpoch {
            serial: Some(1),
            history_epoch: 0,
            items,
        }];
        events
            .try_send(ChatEvent::Snapshot(Box::new((
                snapshot(&chat),
                epochs,
                false,
            ))))
            .unwrap();
        runtime = Some((id, events));
    }
    (task.await.unwrap(), runtime)
}

async fn next(events: &mut Events) -> proto::ThreadEvent {
    tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .expect("an event in time")
        .expect("the stream goes on")
        .expect("no stream error")
}

/// Reads a snapshot to its end: its boundary and the installed items, with
/// fragmented items reassembled and verified as a client must.
async fn read_snapshot(events: &mut Events) -> (proto::Cursor, Vec<proto::TranscriptItem>) {
    use proto::thread_event::Event;
    let Some(Event::SnapshotStart(start)) = next(events).await.event else {
        panic!("a snapshot starts with its boundary");
    };
    let mut items = vec![];
    let mut assembly = vec![];
    loop {
        let event = next(events).await;
        assert!(event.encoded_len() <= host::CONTROL_BYTES);
        assert!(event.cursor.is_none(), "snapshot parts have no live cursor");
        match event.event.unwrap() {
            Event::SnapshotItems(chunk) => items.extend(chunk.items),
            Event::ItemFragment(fragment) => {
                assert!(fragment.data.len() <= 64 * 1024);
                assert_eq!(fragment.offset, assembly.len() as u64);
                assembly.extend_from_slice(&fragment.data);
                if assembly.len() as u64 == fragment.total_bytes {
                    use sha2::{Digest, Sha256};
                    assert_eq!(Sha256::digest(&assembly).to_vec(), fragment.sha256);
                    items.push(proto::TranscriptItem::decode(assembly.as_slice()).unwrap());
                    assembly.clear();
                }
            }
            Event::SnapshotEnd(end) => {
                assert_eq!(end.boundary, start.boundary);
                assert!(assembly.is_empty(), "no partial item is installed");
                return (start.boundary.unwrap(), items);
            }
            other => panic!("unexpected snapshot part {other:?}"),
        }
    }
}

async fn settle(app: &FakeApp, expected: &[Call]) {
    for _ in 0..500 {
        // A retiring stream can report zero observers after Unsubscribe; that
        // notification does not undo the completed cleanup of its subscription.
        if app
            .calls()
            .windows(expected.len())
            .any(|calls| calls == expected)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("expected {expected:?}, got {:?}", app.calls());
}

use prost::Message;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clients_share_one_projection_and_a_reconnect_replays_exactly_what_it_missed() {
    let h = harness(Duration::from_secs(60), Limits::default()).await;
    let (a, runtime) = subscribe(&h, None, Some(vec![text(1, "hel")])).await;
    let (id, runtime) = runtime.unwrap();
    let mut a = a.unwrap();
    let (boundary, items) = read_snapshot(&mut a).await;
    assert_eq!(items.len(), 1);
    // A second client joins the same projection: no new runtime subscription, and
    // the runtime is told it has two observers.
    let (b, created) = subscribe(&h, None, None).await;
    assert!(created.is_none());
    let mut b = b.unwrap();
    let (b_boundary, _) = read_snapshot(&mut b).await;
    assert_eq!(b_boundary, boundary);
    settle(&h.app, &[Call::Observers(id, 2)]).await;

    // A change after the boundary reaches both as one UTF-8 byte-offset append.
    runtime
        .try_send(ChatEvent::Items {
            serial: 1,
            items: vec![(0, text(1, "helló"))],
        })
        .unwrap();
    let first = next(&mut a).await;
    assert_eq!(
        first.cursor.as_ref().unwrap().sequence,
        boundary.sequence + 1
    );
    let Some(proto::thread_event::Event::AppendText(append)) = &first.event else {
        panic!("an append")
    };
    assert_eq!((append.utf8_offset, append.text.as_str()), (3, "ló"));
    assert_eq!(next(&mut b).await, first);

    // Both go; the projection waits for a reconnect and the chat may sleep.
    drop((a, b));
    settle(&h.app, &[Call::Observers(id, 0)]).await;
    runtime
        .try_send(ChatEvent::Items {
            serial: 1,
            items: vec![(1, text(2, "missed"))],
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Reconnecting from the last applied cursor replays only the missed frame.
    let (c, created) = subscribe(&h, first.cursor.clone(), None).await;
    assert!(created.is_none());
    let mut c = c.unwrap();
    let missed = next(&mut c).await;
    assert_eq!(
        missed.cursor.as_ref().unwrap().sequence,
        boundary.sequence + 2
    );
    assert!(matches!(
        missed.event,
        Some(proto::thread_event::Event::ReplaceItem(ref i)) if i.key.as_ref().unwrap().item_id == 2
    ));
    settle(&h.app, &[Call::Observers(id, 1)]).await;
    // A cursor of another stream lifetime gets a whole snapshot, never a gap.
    let mut stale = boundary.clone();
    stale.stream_id = "chat/other".into();
    let (d, _) = subscribe(&h, Some(stale), None).await;
    let (_, items) = read_snapshot(&mut d.unwrap()).await;
    assert_eq!(items.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_projection_closes_after_the_grace_and_a_stopped_runtime_forces_resync() {
    let h = harness(Duration::from_millis(100), Limits::default()).await;
    let (a, runtime) = subscribe(&h, None, Some(vec![text(1, "a")])).await;
    let (id, runtime) = runtime.unwrap();
    let mut a = a.unwrap();
    read_snapshot(&mut a).await;
    drop(a);
    // Grace over: the runtime subscription ends and the stream lifetime closes.
    settle(&h.app, &[Call::Observers(id, 0), Call::Unsubscribe(id)]).await;
    assert!(h.rpc.projections.lock().unwrap().entries.is_empty());
    assert!(runtime.is_closed());

    // A new projection; its runtime replaces the session: clients must resync.
    let (b, runtime) = subscribe(&h, None, Some(vec![text(1, "a")])).await;
    let (id, runtime) = runtime.unwrap();
    let mut b = b.unwrap();
    read_snapshot(&mut b).await;
    runtime.try_send(ChatEvent::SessionReplaced).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), b.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.message().starts_with("resync_required"));
    drop(b);
    settle(&h.app, &[Call::Unsubscribe(id)]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_projection_start_leaves_no_runtime_subscription() {
    let h = harness(Duration::from_secs(60), Limits::default()).await;
    let task = tokio::spawn({
        let (rpc, record, grant) = (h.rpc.clone(), h.record.clone(), h.grant.clone());
        async move { rpc.subscribe(record, grant, None).await }
    });
    let (id, _, events) = h.app.subscribed.recv().await.unwrap();
    // The runtime answers with something other than its snapshot.
    events.try_send(ChatEvent::Stopping).unwrap();
    assert!(task.await.unwrap().is_err());
    settle(&h.app, &[Call::Unsubscribe(id)]).await;
    assert!(h.rpc.projections.lock().unwrap().entries.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_flags_follow_commit_proofs_of_the_exact_version() {
    let h = harness(Duration::from_secs(60), Limits::default()).await;
    let chat = h.record.id().to_owned();
    // Item 1 is stored before the projection starts: it is proven saved at once.
    let stored = turn_end(1);
    h.client
        .save_chat_history_checkpointed(vec![ChatHistoryBatch {
            chat: chat.clone(),
            epochs: vec![(0, "{}".into())],
            items: vec![(0, 1, history::item_json(&stored).unwrap())],
        }])
        .await
        .unwrap();
    let (a, runtime) = subscribe(&h, None, Some(vec![stored, turn_end(2)])).await;
    let (_, runtime) = runtime.unwrap();
    let mut a = a.unwrap();
    let (boundary, items) = read_snapshot(&mut a).await;
    let saved = |item: &proto::TranscriptItem| match &item.body {
        Some(proto::transcript_item::Body::TurnEnd(end)) => end.saved,
        _ => panic!("a turn end"),
    };
    assert_eq!(items.iter().map(saved).collect::<Vec<_>>(), [true, false]);

    // A proof of another version of item 2 proves nothing.
    let mut other = turn_end(2);
    other.turn = t::TurnId(9);
    let wrong = h
        .client
        .save_chat_history_checkpointed(vec![ChatHistoryBatch {
            chat: chat.clone(),
            epochs: vec![],
            items: vec![(0, 2, history::item_json(&other).unwrap())],
        }])
        .await
        .unwrap();
    runtime
        .try_send(ChatEvent::Committed(wrong.into_iter().next().unwrap()))
        .unwrap();
    // The exact version's commit marks the turn end saved, then moves the watermark
    // past the snapshot boundary.
    let right = h
        .client
        .save_chat_history_checkpointed(vec![ChatHistoryBatch {
            chat,
            epochs: vec![],
            items: vec![(0, 2, history::item_json(&turn_end(2)).unwrap())],
        }])
        .await
        .unwrap();
    runtime
        .try_send(ChatEvent::Committed(right.into_iter().next().unwrap()))
        .unwrap();
    let replaced = next(&mut a).await;
    let Some(proto::thread_event::Event::ReplaceItem(item)) = &replaced.event else {
        panic!("the saved turn end, got {replaced:?}")
    };
    assert!(saved(item), "only the matching commit sets saved");
    let watermark = next(&mut a).await;
    assert_eq!(
        watermark.event,
        Some(proto::thread_event::Event::SavedSequence(
            replaced.cursor.unwrap().sequence
        ))
    );
    assert!(replaced_after(&boundary, &watermark));
}
fn replaced_after(boundary: &proto::Cursor, event: &proto::ThreadEvent) -> bool {
    event.cursor.as_ref().unwrap().sequence > boundary.sequence
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_items_stream_lazily_in_bounded_frames_and_reads_are_admitted() {
    let h = harness(Duration::from_secs(60), Limits::default()).await;
    let large = text(2, &"ł".repeat(1024 * 1024));
    let (a, _runtime) = subscribe(&h, None, Some(vec![text(1, "small"), large])).await;
    let mut a = a.unwrap();
    let (_, items) = read_snapshot(&mut a).await;
    assert_eq!(items.len(), 2);
    let Some(proto::transcript_item::Body::Text(block)) = &items[1].body else {
        panic!("the large text")
    };
    assert_eq!(block.text.len(), 2 * 1024 * 1024);
    // Snapshot transfers in progress are bounded per device; one that ends frees
    // its admission at once.
    let mut open = vec![];
    for _ in 0..projection::DEVICE_READS {
        open.push(subscribe(&h, None, None).await.0.unwrap());
    }
    let refused = subscribe(&h, None, None).await.0.err().unwrap();
    assert_eq!(refused.code(), tonic::Code::ResourceExhausted);
    let mut first = open.remove(0);
    read_snapshot(&mut first).await;
    assert!(subscribe(&h, None, None).await.0.is_ok());
    assert!(h.replay.queued_bytes() <= 4 * 1024 * 1024);
}

#[test]
fn private_and_outside_paths_are_presented_without_exposing_host_paths() {
    let dir = tempfile::tempdir().unwrap();
    let client =
        futures_lite::future::block_on(SettingsClient::create(&dir.path().join("s.db"))).unwrap();
    let auth = RpcAuth::new(uuid::Uuid::new_v4().to_string(), client).unwrap();
    let id = ChatSettings::new_id();
    let record = ChatRecord::new(
        id.clone(),
        None,
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            cwd: Some("/data/chats/abc".into()),
            chat: Some(Box::new(ChatSettings {
                id: Some(id),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let context = projection::Context::of(&auth, &record);
    let message = item(
        1,
        t::Body::User(t::UserMessage {
            text: "see".into(),
            delivery: t::Delivery::Sent,
            uuid: None,
            attachments: vec![
                Attachment::Path {
                    path: "/data/chats/abc/notes/plan.md".into(),
                    label: None,
                },
                Attachment::Path {
                    path: "/etc/hosts".into(),
                    label: None,
                },
            ],
            context: None,
            goal: false,
        }),
    );
    // Before: a private chat's path made the whole projection fail.
    let projected = projection::project(&message, 0, 1, 1, &context, &HashMap::new()).unwrap();
    let Some(proto::transcript_item::Body::User(user)) = projected.body else {
        panic!("a user message")
    };
    let paths: Vec<_> = user
        .draft
        .unwrap()
        .attachments
        .into_iter()
        .map(|a| match a.content {
            Some(proto::attachment::Content::File(file)) => {
                (a.label, file.project_file, file.relative_path)
            }
            other => panic!("a path, got {other:?}"),
        })
        .collect();
    assert_eq!(
        paths,
        [
            ("plan.md".to_owned(), false, "notes/plan.md".to_owned()),
            ("hosts".to_owned(), false, String::new()),
        ]
    );
}

fn meta(_h: &Harness, revision: Option<u64>) -> proto::CommandMeta {
    proto::CommandMeta {
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: revision,
    }
}

impl ChatsRpc {
    async fn resolve_draft(
        &self,
        grant: &DeviceGrant,
        record: &ChatRecord,
        draft: proto::MessageDraft,
    ) -> Result<QueueDraft, Status> {
        use canopy_desktop::command_bus::{Command, Control, DraftResources};
        let record = self
            .auth
            .client()
            .save_runtime_chat(record.clone())
            .await
            .map_err(storage_status)?;
        let target = proto::ThreadTarget {
            thread: reference(&self.auth, &record),
            executor_epoch: 0,
        };
        let command = Command::Control(Control::Steer {
            run: 7,
            message: uuid::Uuid::new_v4().to_string(),
            draft: Some(canopy_desktop::api::decode::draft(draft)?),
        });
        let envelope = canopy_desktop::api::decode::envelope(
            proto::CommandMeta {
                command_id: uuid::Uuid::new_v4().to_string(),
                command_epoch: 1,
                expected_revision: None,
            },
            canopy_desktop::api::decode::target(target)?,
            command,
        )?;
        let prepared = CommandBus::new(self.auth.client().clone())
            .prepare(
                envelope,
                Principal::authenticated(grant, 1).map_err(storage_status)?,
            )
            .await
            .map_err(storage_status)?;
        prepared
            .runtime_draft(&DraftResources {
                roots: canopy_desktop::remote::auth::WorkspaceRoots::default(),
                pulls: self.pulls.clone(),
            })
            .map_err(storage_status)?
            .ok_or_else(|| Status::invalid_argument("Missing draft."))
    }
    async fn execute_bus(
        &self,
        grant: DeviceGrant,
        meta: proto::CommandMeta,
        thread: Option<proto::ThreadRef>,
        command: canopy_desktop::command_bus::Command,
        _resources: Option<()>,
    ) -> Result<tonic::Response<proto::CommandReceipt>, Status> {
        let target = proto::ThreadTarget {
            thread,
            executor_epoch: 0,
        };
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let envelope = canopy_desktop::api::decode::envelope(
            meta,
            canopy_desktop::api::decode::target(target.clone())?,
            command,
        )?;
        let receipt = CommandBus::new(self.auth.client().clone())
            .execute(envelope, principal, None)
            .await
            .map_err(storage_status)?;
        Ok(tonic::Response::new(canopy_desktop::api::project_receipt(
            &self.auth,
            &receipt,
            proto::command_receipt::Resource::Target(target),
        )?))
    }
}

#[tokio::test]
async fn local_and_grpc_send_share_durable_results_retry_and_queue_conditions() {
    use canopy_desktop::command_bus::{Command, DraftIntent, QueueChange};
    use canopy_desktop::host::{CommandKey, CommandState};
    let h = harness(Duration::from_secs(5), Limits::default()).await;
    let remote_record = h.client.save_runtime_chat(h.record.clone()).await.unwrap();
    let local_id = ChatSettings::new_id();
    let mut metadata = remote_record.metadata().clone();
    metadata.chat.as_mut().unwrap().id = Some(local_id.clone());
    let local_record = h
        .client
        .save_runtime_chat(
            ChatRecord::new(
                local_id.clone(),
                None,
                remote_record.agent().into(),
                metadata,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let bus = CommandBus::new(h.client.clone());
    let host = canopy_desktop::thread::HostId::new(h.rpc.auth.host_id().into()).unwrap();
    let command_id = uuid::Uuid::new_v4().to_string();
    let message = uuid::Uuid::new_v4().to_string();
    let key = CommandKey::local(command_id.clone()).unwrap();
    let envelope = bus
        .catalog_envelope(
            &key,
            host.clone(),
            local_id.clone(),
            Command::Send {
                message: message.clone(),
                draft: DraftIntent::Ready(QueueDraft::from(Draft::text("same message"))),
                context: local_record.send_context(),
                steer_run: None,
            },
            Conditions::default(),
        )
        .await
        .unwrap();
    let local = bus
        .execute(envelope.clone(), Principal::local_owner(), None)
        .await
        .unwrap();
    let remote_command = Command::Send {
        message: message.clone(),
        draft: canopy_desktop::api::decode::draft(proto::MessageDraft {
            text: "same message".into(),
            ..Default::default()
        })
        .unwrap(),
        context: remote_record.send_context(),
        steer_run: None,
    };
    let mut remote_meta = meta(&h, None);
    remote_meta.command_id = command_id;
    let chat = reference(&h.rpc.auth, &remote_record);
    let remote = h
        .rpc
        .execute_bus(
            h.grant.clone(),
            remote_meta.clone(),
            chat.clone(),
            remote_command.clone(),
            None,
        )
        .await
        .unwrap()
        .into_inner();
    assert_eq!(local.state(), CommandState::Succeeded);
    assert_eq!(remote.state, proto::OperationState::Succeeded as i32);
    let Some(proto::command_receipt::Result::Send(sent)) = remote.result.as_ref() else {
        panic!("send receipt");
    };
    assert_eq!(local.result(), Some(sent.message_id.as_bytes()));
    let local_queue = h.client.load_runtime_queue(local_id.clone()).await.unwrap();
    let remote_queue = h
        .client
        .load_runtime_queue(remote_record.id().into())
        .await
        .unwrap();
    assert_eq!(
        local_queue, remote_queue,
        "both adapters commit the same domain queue"
    );

    // Retrying either adapter returns its receipt without accepting a second item.
    assert_eq!(
        bus.execute(envelope, Principal::local_owner(), None)
            .await
            .unwrap(),
        local
    );
    assert_eq!(
        h.rpc
            .execute_bus(
                h.grant.clone(),
                remote_meta.clone(),
                chat.clone(),
                remote_command.clone(),
                None
            )
            .await
            .unwrap()
            .into_inner(),
        remote
    );
    assert_eq!(
        h.client
            .load_runtime_queue(local_id.clone())
            .await
            .unwrap()
            .entries()
            .len(),
        1
    );
    assert_eq!(
        h.client
            .load_runtime_queue(remote_record.id().into())
            .await
            .unwrap()
            .entries()
            .len(),
        1
    );

    let stale = Command::Queue {
        revision: local_queue.revision().saturating_sub(1),
        change: QueueChange::Pause { paused: true },
    };
    let envelope = bus
        .catalog_envelope(
            &CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap(),
            host,
            local_id.clone(),
            stale.clone(),
            Conditions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        bus.execute(envelope, Principal::local_owner(), None)
            .await
            .unwrap()
            .state(),
        canopy_desktop::host::CommandState::Rejected
    );
    let conflict = h
        .rpc
        .execute_bus(h.grant.clone(), meta(&h, None), chat.clone(), stale, None)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(conflict.state, proto::OperationState::Rejected as i32);
    assert_eq!(
        h.client.load_runtime_queue(local_id).await.unwrap(),
        local_queue
    );
    assert_eq!(
        h.client
            .load_runtime_queue(remote_record.id().into())
            .await
            .unwrap(),
        remote_queue
    );

    // Current authorization still precedes receipt disclosure on a retry.
    h.client
        .revoke_runtime_device(h.grant.id().into())
        .await
        .unwrap();
    let denied = h
        .rpc
        .execute_bus(h.grant.clone(), remote_meta, chat, remote_command, None)
        .await
        .unwrap_err();
    assert_eq!(denied.code(), tonic::Code::PermissionDenied);
    h.client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_reused_command_id_with_changed_arguments_conflicts_instead_of_answering_the_old_receipt()
{
    let h = harness(Duration::from_secs(5), Limits::default()).await;
    let record = h.client.save_runtime_chat(h.record.clone()).await.unwrap();
    let target = proto::ThreadTarget {
        thread: reference(&h.rpc.auth, &record),
        executor_epoch: 0,
    };
    let meta = meta(&h, None);
    let envelope = canopy_desktop::api::decode::envelope(
        meta.clone(),
        canopy_desktop::api::decode::target(target).unwrap(),
        canopy_desktop::command_bus::Command::Control(
            canopy_desktop::command_bus::Control::Interrupt { run: 7 },
        ),
    )
    .unwrap();
    let bus = CommandBus::new(h.client.clone());
    let principal = Principal::authenticated(&h.grant, meta.command_epoch).unwrap();
    let prepared = bus
        .prepare(envelope.clone(), principal.clone())
        .await
        .unwrap();
    let first = prepared.fingerprint().to_vec();
    h.client
        .accept_runtime_command(prepared.runtime_input().clone(), host::CHAT)
        .await
        .unwrap();

    let retry = bus
        .prepare(envelope.clone(), principal.clone())
        .await
        .unwrap();
    let receipt = retry.prior().unwrap();
    assert_eq!(receipt.state(), host::CommandState::Accepted);
    assert_eq!(receipt.key().id(), meta.command_id);

    // Before: the same ID with another run answered the old receipt as if done.
    let mut changed = envelope.clone();
    changed.command = canopy_desktop::command_bus::Command::Control(
        canopy_desktop::command_bus::Control::Interrupt { run: 8 },
    );
    let error = bus.prepare(changed, principal.clone()).await.err().unwrap();
    assert!(matches!(
        error,
        canopy_desktop::settings::Error::CommandConflict
    ));

    // The expected revision is part of what a command asks.
    let mut revised = meta.clone();
    revised.expected_revision = Some(3);
    let mut changed = envelope.clone();
    changed.conditions.expected_revision = revised.expected_revision;
    assert!(matches!(
        bus.prepare(changed, principal.clone()).await.err().unwrap(),
        canopy_desktop::settings::Error::CommandConflict
    ));

    // The transport identity is not: another ID, epoch or host spelling of the same
    // request has the same fingerprint.
    let mut other = meta.clone();
    other.command_id = uuid::Uuid::new_v4().to_string();
    let mut independent = envelope;
    independent.command_id = uuid::Uuid::parse_str(&other.command_id).unwrap();
    assert_eq!(
        bus.prepare(independent, principal)
            .await
            .unwrap()
            .fingerprint(),
        first
    );
}

#[tokio::test]
async fn a_pull_request_attachment_needs_the_hosts_signed_context_for_this_checkout() {
    let h = harness(Duration::from_secs(5), Limits::default()).await;
    let pull = proto::PullAttachment {
        number: 12,
        title: "Fix login".into(),
        url: "https://github.com/acme/app/pull/12".into(),
        head: "fix".into(),
        base: "main".into(),
        state: "open".into(),
        body: String::new(),
        context_id: None,
    };
    let draft = |pull: proto::PullAttachment| proto::MessageDraft {
        text: "Review".into(),
        attachments: vec![proto::Attachment {
            label: "#12".into(),
            content: Some(proto::attachment::Content::Pull(pull)),
        }],
        context: None,
        goal: false,
    };
    let error = h
        .rpc
        .resolve_draft(&h.grant, &h.record, draft(pull.clone()))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Aborted);

    let signed = h.rpc.pulls.publish(h.record.authority_root(), pull.clone());
    let accepted = h
        .rpc
        .resolve_draft(&h.grant, &h.record, draft(signed.clone()))
        .await
        .unwrap();
    assert!(matches!(
        accepted.attachments(),
        [Attachment::Pull { number: 12, title, .. }] if title == "Fix login"
    ));

    // Changed after signing, or signed for another checkout: refused.
    let mut changed = signed.clone();
    changed.title = "Something else".into();
    assert!(
        h.rpc
            .resolve_draft(&h.grant, &h.record, draft(changed))
            .await
            .is_err()
    );
    let elsewhere = h.rpc.pulls.publish("/work/other", pull);
    assert!(
        h.rpc
            .resolve_draft(&h.grant, &h.record, draft(elsewhere))
            .await
            .is_err()
    );
}
