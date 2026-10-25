//! What a device sees of the host catalog and which changes it is sent.
use super::*;
use crate::app_state::chats::ChatSummary;
use canopy_desktop::remote::host::HostState;
use canopy_desktop::{
    chat::config::ChatSettings,
    host::DeviceRegistration,
    settings::SettingsClient,
    state::workspace::{PaneKind, PaneMetadata},
};

fn record(workspace: Option<&str>, root: &str) -> ChatRecord {
    let id = ChatSettings::new_id();
    let record = ChatRecord::new(
        id.clone(),
        workspace.map(str::to_owned),
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            profile_id: Some("claude-default".into()),
            chat: Some(Box::new(ChatSettings {
                id: Some(id),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    match workspace {
        Some(_) => record.with_authority_root(root.into()).unwrap(),
        None => record,
    }
}
fn summary(record: &ChatRecord, working: bool) -> ChatSummary {
    ChatSummary {
        chat: record.id().into(),
        workspace: record.workspace().map(str::to_owned),
        tool: "claude".into(),
        title: Some("Plan".into()),
        status: "Ready",
        working,
        decisions: 0,
        unsent: 0,
        observers: 0,
        recovery: None,
    }
}

#[tokio::test]
async fn a_device_sees_only_its_roots_and_then_only_what_changed() {
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&dir.path().join("s.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "phone".into(),
                vec![3; 32],
                host::READ,
                vec!["/work/a".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![3; 32]).await.unwrap();
    let auth = RpcAuth::new(host.id().into(), client).unwrap();
    let (a, b) = (WorkspaceId::new(), WorkspaceId::new());
    let mine = record(Some(&workspace_ref(a)), "/work/a");
    let other = record(Some(&workspace_ref(b)), "/work/b");
    let private = record(None, "private");
    let workspaces = vec![
        WorkspaceSummary {
            workspace: a,
            name: "a".into(),
            canonical_root: "/work/a".into(),
            repository: None,
            closing: false,
        },
        WorkspaceSummary {
            workspace: b,
            name: "b".into(),
            canonical_root: "/work/b".into(),
            repository: None,
            closing: false,
        },
    ];
    let event = |working| CatalogEvent {
        workspaces: workspaces.clone(),
        chats: [&mine, &other, &private]
            .into_iter()
            .map(|r| summary(r, working))
            .collect(),
        records: vec![mine.clone(), other.clone(), private.clone()],
    };
    let first = catalog(&auth, &grant, &event(false), &[]);
    // Neither another checkout's project and chats nor Private chats are listed.
    assert_eq!(first.projects.len(), 1);
    assert_eq!(first.projects[0].checkouts[0].path, "/work/a");
    assert_eq!(first.chats.len(), 1);
    assert_eq!(first.chats[0].chat.as_ref().unwrap().chat_id, mine.id());

    use proto::summary_event::Change;
    use watch::Plan;
    let (plan, sent) = watch::diff(None, &first);
    assert!(matches!(plan, Plan::Snapshot));
    // A status change of a visible chat is one chat delta; invisible ones send nothing.
    let (plan, sent) = watch::diff(Some(&sent), &catalog(&auth, &grant, &event(true), &[]));
    assert!(
        matches!(&plan, Plan::Deltas(c) if matches!(c.as_slice(), [Change::Chat(c)] if c.state == proto::ChatState::Working as i32))
    );
    // A deleted chat is reported as deleted.
    let mut gone = event(true);
    gone.chats.retain(|c| c.chat != mine.id());
    let (plan, sent) = watch::diff(Some(&sent), &catalog(&auth, &grant, &gone, &[]));
    assert!(matches!(
        &plan,
        Plan::Deltas(c) if matches!(c.as_slice(), [Change::Deleted(proto::ResourceRef { resource: Some(proto::resource_ref::Resource::Chat(c)) })]
            if c.chat_id == mine.id())
    ));
    // A project change resends the whole catalog.
    let mut closed = gone;
    closed.workspaces.retain(|w| w.workspace != a);
    let (plan, _) = watch::diff(Some(&sent), &catalog(&auth, &grant, &closed, &[]));
    assert!(matches!(plan, Plan::Snapshot));
}

/// About 2100 chats with 512-byte titles and a project with thousands of checkouts:
/// well over one frame, as unary `ListResources` cannot carry it.
fn large_catalog() -> proto::Catalog {
    let checkout = |i: usize| proto::WorkspaceSummary {
        workspace: Some(proto::WorkspaceRef {
            host_id: "h".into(),
            workspace_id: i.to_string(),
        }),
        name: format!("tree-{i}"),
        path: format!("/work/repo/.worktrees/{}-{i}", "x".repeat(200)),
        closing: Some(false),
        repository_path: Some("/work/repo".into()),
    };
    proto::Catalog {
        host_id: "h".into(),
        projects: vec![
            proto::ProjectSummary {
                project_id: "/work/repo".into(),
                name: "repo".into(),
                path: "/work/repo".into(),
                checkouts: (0..3000).map(checkout).collect(),
            },
            proto::ProjectSummary {
                project_id: "/work/empty".into(),
                name: "empty".into(),
                path: "/work/empty".into(),
                checkouts: vec![],
            },
        ],
        chats: (0..2100)
            .map(|i| proto::ChatSummary {
                chat: Some(proto::ChatRef {
                    host_id: "h".into(),
                    chat_id: format!("chat-{i}"),
                }),
                title: format!("{i:04}{}", "t".repeat(508)),
                agent_id: "claude".into(),
                ..Default::default()
            })
            .collect(),
        terminals: (0..50)
            .map(|i| proto::TerminalSummary {
                terminal: Some(proto::TerminalRef {
                    host_id: "h".into(),
                    terminal_id: format!("term-{i}"),
                }),
                title: "zsh".into(),
                ..Default::default()
            })
            .collect(),
    }
}

fn cursor(sequence: u64) -> proto::Cursor {
    proto::Cursor {
        runtime_epoch: "epoch".into(),
        stream_id: "catalog/test".into(),
        sequence,
    }
}

#[test]
fn a_catalog_over_one_frame_arrives_whole_in_bounded_pages() {
    use canopy_desktop::remote::client::catalog::CatalogStaging;
    let whole = large_catalog();
    assert!(whole.encoded_len() > pages::FRAME_LIMIT);
    let hub = canopy_desktop::remote::sync::ReplayHub::new(Default::default());
    let projection = pages::Projection::charge(&hub, whole.clone()).unwrap();
    let mut pager = pages::Pager::new(projection, cursor(1));
    let mut staging = CatalogStaging::default();
    let mut parts = 0;
    let installed = loop {
        let part = pager
            .next_part()
            .unwrap()
            .expect("a last page ends the snapshot");
        parts += 1;
        let page = part.page.as_ref().unwrap();
        assert!(page.encoded_len() <= pages::PAGE_TARGET + 1024);
        assert_eq!(part.first, parts == 1);
        // Nothing is installed before the last page.
        if let Some((boundary, catalog)) = staging.push("h", part).unwrap() {
            assert_eq!(boundary, cursor(1));
            break catalog;
        }
    };
    assert!(parts > 16, "split into many pages, not one frame");
    assert!(pager.next_part().unwrap().is_none());
    // Every row, the split project's metadata and checkout order preserved.
    assert_eq!(installed, whole);
}

#[test]
fn the_projection_budget_is_finite_and_released() {
    let hub = canopy_desktop::remote::sync::ReplayHub::new(Default::default());
    let mut huge = large_catalog();
    let chats = huge.chats.clone();
    for _ in 0..40 {
        huge.chats.extend(chats.iter().cloned());
    }
    assert!(huge.encoded_len() > 32 * 1024 * 1024);
    let error = pages::Projection::charge(&hub, huge).err().unwrap();
    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    // Held projections share one host budget; dropping one frees its share.
    let held: Vec<_> = (0..64)
        .map_while(|_| pages::Projection::charge(&hub, large_catalog()).ok())
        .collect();
    assert!(held.len() < 64, "projections beyond 32 MiB are refused");
    let count = held.len();
    drop(held);
    assert!(pages::Projection::charge(&hub, large_catalog()).is_ok());
    assert!(count > 0);
}

#[tokio::test]
async fn a_changed_authorization_drops_what_was_queued_before_the_next_event() {
    use proto::summary_event::Change;
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&dir.path().join("s.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let device = uuid::Uuid::new_v4().to_string();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                device.clone(),
                "phone".into(),
                vec![4; 32],
                host::READ,
                vec!["/work/a".into(), "/work/b".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
    let state = HostState::new(host.id(), vec![]);
    state.set(proto::HostReadiness::Ready, String::new());
    let mut status = state.watch();
    let hub = canopy_desktop::remote::sync::ReplayHub::new(Default::default());
    let (a, b) = (WorkspaceId::new(), WorkspaceId::new());
    let in_a = record(Some(&workspace_ref(a)), "/work/a");
    let in_b = record(Some(&workspace_ref(b)), "/work/b");
    let workspace = |id, root: &str| WorkspaceSummary {
        workspace: id,
        name: root.into(),
        canonical_root: root.into(),
        repository: None,
        closing: false,
    };
    let event = |working| CatalogEvent {
        workspaces: vec![workspace(a, "/work/a"), workspace(b, "/work/b")],
        chats: vec![summary(&in_a, working), summary(&in_b, working)],
        records: vec![in_a.clone(), in_b.clone()],
    };
    let mut queue = watch::Queue::new("catalog/test".into());
    let grant = watch::authorize(&auth, &mut status, &[4; 32])
        .await
        .unwrap();
    let project = |grant: &DeviceGrant, event: &CatalogEvent| {
        pages::Projection::build(&hub, &auth, grant, event, &[]).unwrap()
    };
    let first = project(&grant, &event(false));
    queue.queue(&hub, grant.clone(), first, "epoch").unwrap();
    assert!(queue.admit(&grant));
    assert!(matches!(
        queue.take("epoch").unwrap().unwrap().change,
        Some(Change::Snapshot(_))
    ));
    // Both chats start working: two deltas wait.
    queue
        .queue(&hub, grant.clone(), project(&grant, &event(true)), "epoch")
        .unwrap();
    assert!(queue.admit(&grant));
    queue.take("epoch").unwrap().unwrap();
    assert!(queue.pending());
    // Root /work/b is withdrawn between the two queued events.
    client
        .update_runtime_device_authorization(
            device.clone(),
            grant.authorization_revision(),
            host::READ,
            vec!["/work/a".into()],
        )
        .await
        .unwrap();
    let reduced = watch::authorize(&auth, &mut status, &[4; 32])
        .await
        .unwrap();
    assert!(
        !queue.admit(&reduced),
        "the queued delta was for the old grant"
    );
    assert!(
        !queue.pending(),
        "nothing projected for the old grant is left"
    );
    queue
        .queue(
            &hub,
            reduced.clone(),
            project(&reduced, &event(true)),
            "epoch",
        )
        .unwrap();
    assert!(queue.admit(&reduced));
    let Some(Change::Snapshot(resync)) = queue.take("epoch").unwrap().unwrap().change else {
        panic!("a changed authorization resends the whole catalog");
    };
    assert_eq!(resync.chats.len(), 1);
    assert_eq!(resync.chats[0].chat.as_ref().unwrap().chat_id, in_a.id());
    assert!(!queue.pending());
    // Revoked with an event queued: the next authorization fails before it is sent.
    queue
        .queue(
            &hub,
            reduced.clone(),
            project(&reduced, &event(false)),
            "epoch",
        )
        .unwrap();
    assert!(queue.pending());
    client.revoke_runtime_device(device).await.unwrap();
    let refused = watch::authorize(&auth, &mut status, &[4; 32]).await;
    assert!(refused.is_err());
}

/// Thousands of worktrees of one repository and chats with long titles, as the app
/// host lists them, visible to `grant`'s roots.
fn large_event() -> CatalogEvent {
    let workspaces: Vec<_> = (0..3000)
        .map(|i| WorkspaceSummary {
            workspace: WorkspaceId::new(),
            name: format!("tree-{i}"),
            canonical_root: format!("/work/repo/.worktrees/{}-{i}", "x".repeat(200)).into(),
            repository: Some("/work/repo".into()),
            closing: false,
        })
        .collect();
    let records: Vec<_> = (0..2100)
        .map(|i| {
            record(
                Some(&workspace_ref(workspaces[i].workspace)),
                "/work/repo/.worktrees",
            )
        })
        .collect();
    let chats = records
        .iter()
        .map(|r| ChatSummary {
            title: Some("t".repeat(512)),
            ..summary(r, false)
        })
        .collect();
    CatalogEvent {
        workspaces,
        chats,
        records,
    }
}

#[tokio::test]
async fn a_full_budget_refuses_a_projection_before_it_is_built() {
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&dir.path().join("s.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "phone".into(),
                vec![5; 32],
                host::READ,
                vec!["*".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![5; 32]).await.unwrap();
    let auth = RpcAuth::new(host.id().into(), client).unwrap();
    let event = large_event();
    let estimate = pages::estimate(&auth, &grant, &event, &[]);
    // The estimate covers what the build allocates, so nothing is copied unadmitted.
    let built = catalog(&auth, &grant, &event, &[]);
    assert!(built.encoded_len() > pages::FRAME_LIMIT);
    assert!(estimate >= pages::footprint(&built));
    assert!(
        estimate < 8 * pages::footprint(&built),
        "not the whole budget"
    );
    drop(built);

    // Other devices' projections hold all but less than this one needs.
    let hub = canopy_desktop::remote::sync::ReplayHub::new(Default::default());
    let others = hub
        .reserve_assembly(32 * 1024 * 1024 - estimate + 1)
        .unwrap();
    let mut copied = false;
    let refused = pages::Projection::admit(&hub, estimate, || {
        copied = true;
        catalog(&auth, &grant, &event, &[])
    });
    assert_eq!(
        refused.err().unwrap().code(),
        tonic::Code::ResourceExhausted
    );
    assert!(!copied, "refused before the per-device copy");
    drop(others);

    // Admitted, its 3000 checkouts are paged without a page holding all of them.
    use canopy_desktop::remote::client::catalog::CatalogStaging;
    let projection = pages::Projection::build(&hub, &auth, &grant, &event, &[]).unwrap();
    let whole = projection.catalog.clone();
    assert_eq!(whole.projects[0].checkouts.len(), 3000);
    let mut pager = pages::Pager::new(projection, cursor(1));
    let mut staging = CatalogStaging::default();
    let installed = loop {
        let part = pager.next_part().unwrap().expect("a last page");
        let page = part.page.as_ref().unwrap();
        assert!(pages::footprint(page) < 4 * pages::PAGE_TARGET);
        if let Some((_, catalog)) = staging.push(host.id(), part).unwrap() {
            break catalog;
        }
    };
    assert_eq!(installed, whole);
}
