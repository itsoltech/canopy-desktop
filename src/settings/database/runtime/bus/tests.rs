use super::*;
use crate::{
    chat::config::ChatSettings,
    command_bus::{Conditions, DraftIntent, QueueChange},
    state::workspace::{PaneKind, PaneMetadata},
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn local_recovery_retains_selected_authority_for_exact_retry_and_rejects_a_loser() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recovery.db");
    crate::settings::database::create(&path).unwrap();
    let mut db = Database::open(&path, Access::ReadWrite).unwrap();
    let chat = id();
    db.save_chat_history(vec![crate::settings::ChatHistoryBatch {
        chat: chat.clone(),
        epochs: vec![(7, "{}".into())],
        items: vec![(7, 1, "retained history".into())],
    }])
    .unwrap();
    db.initialize_runtime().unwrap();
    let orphan = db.load_runtime_chat(chat.clone()).unwrap().unwrap();
    assert_eq!(orphan.authority_root(), "");
    let reason = orphan.recovery_reason().unwrap().to_owned();
    let catalog = super::super::super::read_tools(&db.db).unwrap();
    let tool = catalog
        .tools
        .iter()
        .find(|tool| tool.enabled && tool.is_agent())
        .unwrap();
    let profile = tool
        .profiles
        .first()
        .map_or_else(|| tool.id.clone(), |p| p.id.clone());
    let record = ChatRecord::new(
        chat.clone(),
        None,
        tool.id.clone(),
        PaneMetadata {
            kind: PaneKind::Chat,
            profile_id: Some(profile),
            chat: Some(Box::new(ChatSettings {
                id: Some(chat.clone()),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap()
    .with_history_epoch(7)
    .unwrap();
    let request = envelope(
        &orphan,
        uuid::Uuid::new_v4(),
        Command::Recovery {
            record: Box::new(record.clone()),
            expected_reason: reason.clone(),
        },
    );
    let (_, remote) = principal(&mut db);
    assert!(matches!(
        db.prepare_bus_command(request.clone(), remote),
        Err(Error::NotAuthorized)
    ));
    let winner = db
        .prepare_bus_command(request.clone(), Principal::local_owner())
        .unwrap()
        .input;
    let mut other = request.clone();
    other.command_id = uuid::Uuid::new_v4();
    let loser = db
        .prepare_bus_command(other, Principal::local_owner())
        .unwrap()
        .input;
    let receipt = db
        .recover_runtime_chat(winner.clone(), record.clone(), reason.clone())
        .unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    assert_eq!(
        db.recover_runtime_chat(winner, record.clone(), reason.clone())
            .unwrap(),
        receipt
    );
    assert_eq!(
        db.prepare_bus_command(request, Principal::local_owner())
            .unwrap()
            .prior(),
        Some(&receipt)
    );
    assert_eq!(
        db.recover_runtime_chat(loser, record, reason)
            .unwrap()
            .state(),
        CommandState::Rejected
    );
    let recovered = db.load_runtime_chat(chat.clone()).unwrap().unwrap();
    assert_eq!(recovered.thread_id(), orphan.thread_id());
    assert!(recovered.recovery_reason().is_none());
    assert_eq!(
        db.load_chat_history(&chat).unwrap().items,
        vec![(7, 1, "retained history".into())]
    );
}

#[test]
fn restart_rejects_ownerless_catalog_admission_and_preserves_unknown_effects() {
    use crate::{command_bus::CatalogCommand, state::workspace::ProjectId, thread::ProjectRef};
    let (dir, mut db) = database();
    let source = chat(&mut db);
    let (_, principal) = principal(&mut db);
    let environment = ResourceRef::Environment(crate::thread::EnvironmentRef {
        host_id: source.thread().unwrap().thread().host_id.clone(),
    });
    let project = ResourceRef::Project(
        ProjectRef::new(
            source.thread().unwrap().thread().host_id.clone(),
            ProjectId::new(),
            None,
            false,
        )
        .unwrap(),
    );
    let root = dir.path().to_str().unwrap().to_owned();
    let mut requests = Vec::new();
    for (resource, command) in [
        (
            environment.clone(),
            CatalogCommand::Create {
                agent: "test".into(),
                profile: "test".into(),
            },
        ),
        (
            ResourceRef::Thread {
                thread: source.thread().unwrap().thread().clone(),
                executor_epoch: source.thread().unwrap().executor_epoch(),
            },
            CatalogCommand::Close { plan: id() },
        ),
        (
            environment,
            CatalogCommand::OpenProject { path: root.clone() },
        ),
        (project.clone(), CatalogCommand::CloseProject { plan: id() }),
        (
            project,
            CatalogCommand::CloseTerminal {
                terminal: id(),
                plan: id(),
            },
        ),
    ] {
        for attempting in [false, true] {
            let request = CommandEnvelope {
                command_id: uuid::Uuid::new_v4(),
                resource: resource.clone(),
                command: Command::Catalog(command.clone()),
                conditions: Conditions::default(),
            };
            let prepared = if matches!(request.resource, ResourceRef::Thread { .. }) {
                db.prepare_bus_command(request.clone(), principal.clone())
                    .unwrap()
            } else {
                db.prepare_bus_catalog(
                    request.clone(),
                    principal.clone(),
                    Some((root.clone(), root.clone())),
                )
                .unwrap()
                .unwrap()
            };
            let scope = request.command.required_scope();
            let accepted = db.accept_runtime_command(prepared.input, scope).unwrap();
            if attempting {
                db.transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .unwrap();
            }
            requests.push((request, accepted.key().clone(), scope, attempting));
        }
    }
    // Re-initializing the current owner must not cancel its in-flight admission.
    db.initialize_runtime().unwrap();
    for (_, key, scope, attempting) in &requests {
        assert_eq!(
            db.get_runtime_command(key.clone(), *scope)
                .unwrap()
                .unwrap()
                .state(),
            if *attempting {
                CommandState::Attempting
            } else {
                CommandState::Accepted
            }
        );
    }
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    for (request, key, scope, attempting) in requests {
        let prepared = if matches!(request.resource, ResourceRef::Thread { .. }) {
            db.prepare_bus_command(request.clone(), principal.clone())
                .unwrap()
        } else {
            // Exact retry needs neither a live project nor the old preflight.
            db.prepare_bus_catalog(request.clone(), principal.clone(), None)
                .unwrap()
                .unwrap()
        };
        let receipt = prepared.prior().unwrap();
        assert_eq!(
            receipt.state(),
            if attempting {
                CommandState::DeliveryUnknown
            } else {
                CommandState::Rejected
            }
        );
        if !attempting {
            assert!(
                std::str::from_utf8(receipt.result().unwrap())
                    .unwrap()
                    .contains("conditions_changed")
            );
        }
        assert_eq!(
            db.get_runtime_command(key, scope).unwrap().as_ref(),
            Some(receipt)
        );
    }
}

#[test]
fn projectless_terminal_requires_the_in_process_owner_even_with_private_grants() {
    let (_dir, mut db) = database();
    let (_, remote) = principal(&mut db);
    for command in [
        Command::Terminal(crate::command_bus::TerminalCommand::Open {
            tool: "shell".into(),
            profile: None,
        }),
        Command::Catalog(crate::command_bus::CatalogCommand::CloseTerminal {
            terminal: id(),
            plan: id(),
        }),
    ] {
        let request = CommandEnvelope {
            command_id: uuid::Uuid::new_v4(),
            resource: ResourceRef::Environment(crate::thread::EnvironmentRef {
                host_id: crate::thread::HostId::new(db.runtime_host().unwrap().id().into())
                    .unwrap(),
            }),
            command,
            conditions: Conditions::default(),
        };
        assert!(matches!(
            db.prepare_bus_command(request.clone(), remote.clone()),
            Err(Error::NotAuthorized)
        ));
        let admitted = db
            .prepare_bus_command(request.clone(), Principal::local_owner())
            .unwrap();
        db.accept_runtime_command(admitted.input.clone(), host::TERMINAL)
            .unwrap();
        // Remote grants cannot disclose or conflict with the local intent either.
        assert!(matches!(
            db.prepare_bus_command(request.clone(), remote.clone()),
            Err(Error::NotAuthorized)
        ));
        let retry = db
            .prepare_bus_command(request, Principal::local_owner())
            .unwrap();
        assert_eq!(retry.prior().unwrap().state(), CommandState::Accepted);
    }
}

#[test]
fn git_admission_and_dispatch_recheck_all_roots_and_refuse_missing_saved_authority() {
    use crate::{
        command_bus::{
            GitCommand, GitIntent, GitSnapshot, WorktreeAction, WorktreeIntent, git::SavedAuthority,
        },
        state::workspace::ProjectId,
        thread::{HostId, ProjectRef, WorktreeId},
    };
    let (dir, mut db) = database();
    let source = dir.path().join("source").to_str().unwrap().to_owned();
    let target = dir
        .path()
        .join("removed-target")
        .to_str()
        .unwrap()
        .to_owned();
    let (device, principal) = principal(&mut db);
    let request = CommandEnvelope {
        command_id: uuid::Uuid::new_v4(),
        resource: ResourceRef::Project(
            ProjectRef::new(
                HostId::new(db.runtime_host().unwrap().id().into()).unwrap(),
                ProjectId::new(),
                Some(WorktreeId::new("checkout".into()).unwrap()),
                true,
            )
            .unwrap(),
        ),
        command: Command::Git(GitCommand {
            intent: GitIntent::Worktree(WorktreeIntent {
                action: WorktreeAction::Merge,
                branch: "feature".into(),
                path: None,
                start_point: None,
                target_branch: Some("main".into()),
                delete_branch: false,
                discard_changes: false,
                delete_unmerged_branch: false,
                acknowledge_offline_dirty: false,
                registration_name: None,
            }),
            expected: GitSnapshot {
                head: String::new(),
                index: [0; 32],
                worktree: [0; 32],
                preflight: Some(id()),
            },
        }),
        conditions: Conditions::default(),
    };
    let prepared = db
        .prepare_bus_catalog(
            request.clone(),
            principal.clone(),
            Some((source.clone(), source.clone())),
        )
        .unwrap()
        .unwrap();
    let mut input = prepared.input;
    let raw_intent = input.arguments.clone();
    SavedAuthority::bind(&mut input, vec![target.clone()]).unwrap();
    let grant = db.lookup_runtime_device(device.as_bytes().into()).unwrap();
    let restricted = db
        .update_runtime_device_authorization(
            device.clone(),
            grant.authorization_revision(),
            host::ALL_SCOPES,
            vec![source.clone()],
        )
        .unwrap();
    assert!(matches!(
        db.accept_runtime_command(input.clone(), host::FILES_GIT),
        Err(Error::NotAuthorized)
    ));
    assert!(
        db.get_runtime_command(input.key.clone(), host::FILES_GIT)
            .unwrap()
            .is_none()
    );
    let granted = db
        .update_runtime_device_authorization(
            device.clone(),
            restricted.authorization_revision(),
            host::ALL_SCOPES,
            vec![source.clone(), target.clone()],
        )
        .unwrap();
    db.accept_runtime_command(input.clone(), host::FILES_GIT)
        .unwrap();
    let recovered = db
        .prepare_bus_catalog(request.clone(), principal.clone(), None)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.prior().unwrap().state(), CommandState::Accepted);
    assert_eq!(recovered.input.arguments, input.arguments);
    // Invalid historical payloads never fall back to source-only authorization,
    // including while the device still has every current grant.
    for arguments in [raw_intent, b"{\"version\":1,\"roots\":[]}".to_vec()] {
        db.db
            .execute(
                "UPDATE _canopy_command_inbox SET arguments=?1 WHERE command_id=?2",
                params![arguments, input.key.id()],
            )
            .unwrap();
        assert!(matches!(
            db.get_runtime_command(input.key.clone(), host::FILES_GIT),
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            db.transition_runtime_command(
                input.key.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None
            ),
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            db.prepare_bus_catalog(request.clone(), principal.clone(), None),
            Err(Error::NotAuthorized)
        ));
    }
    db.db
        .execute(
            "UPDATE _canopy_command_inbox SET arguments=?1 WHERE command_id=?2",
            params![input.arguments, input.key.id()],
        )
        .unwrap();
    db.update_runtime_device_authorization(
        device,
        granted.authorization_revision(),
        host::ALL_SCOPES,
        vec![source],
    )
    .unwrap();
    assert!(matches!(
        db.transition_runtime_command(
            input.key,
            CommandState::Accepted,
            CommandState::Attempting,
            None
        ),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn typed_steer_and_goal_use_the_existing_queue_and_dedupe_before_target_resolution() {
    use crate::command_bus::Control;
    let (_dir, mut db) = database();
    let record = chat(&mut db);
    let (_, principal) = principal(&mut db);
    let message = id();
    let draft = QueueDraft::from(crate::chat::session::Draft::text("steered text"));
    let request = envelope(
        &record,
        uuid::Uuid::new_v4(),
        Command::Control(Control::Steer {
            run: 7,
            message: message.clone(),
            draft: Some(DraftIntent::Ready(draft.clone())),
        }),
    );
    let prepared = db
        .prepare_bus_command(request.clone(), principal.clone())
        .unwrap();
    let receipt = db
        .accept_runtime_message(prepared.input, record.id().into(), message.clone(), draft)
        .unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    let queue = db.load_runtime_queue(record.id().into()).unwrap();
    assert_eq!(queue.entries()[0].steer_run(), Some(7));
    let goal_message = id();
    let goal = envelope(
        &record,
        uuid::Uuid::new_v4(),
        Command::Control(Control::SetGoal {
            objective: "finish the task".into(),
        }),
    );
    let prepared = db.prepare_bus_command(goal, principal.clone()).unwrap();
    db.accept_runtime_message(
        prepared.input,
        record.id().into(),
        goal_message,
        QueueDraft::from(crate::chat::session::Draft {
            text: "finish the task".into(),
            attachments: vec![],
            context: None,
            goal: true,
        }),
    )
    .unwrap();
    assert_eq!(
        db.load_runtime_queue(record.id().into())
            .unwrap()
            .entries()
            .len(),
        2
    );
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [record.id()],
        )
        .unwrap();
    let prior = db
        .prepare_bus_command(request, principal)
        .unwrap()
        .prior
        .unwrap();
    assert_eq!(prior.result(), Some(message.as_bytes()));
}

#[test]
fn runtime_acceptance_rechecks_revision_and_commits_its_rejection_before_retry() {
    use crate::command_bus::Control;
    let (_dir, mut db) = database();
    let record = chat(&mut db);
    let (_, principal) = principal(&mut db);
    let mut request = envelope(
        &record,
        uuid::Uuid::new_v4(),
        Command::Control(Control::Edit {
            run: 1,
            history_epoch: 0,
            item: 1,
            draft: incoming("replacement"),
            restore_files: false,
        }),
    );
    request.conditions.expected_revision = Some(record.thread().unwrap().revision());
    let prepared = db
        .prepare_bus_command(request.clone(), principal.clone())
        .unwrap();
    execute(
        &mut db,
        rename(&record, uuid::Uuid::new_v4(), "changed before acceptance"),
        principal.clone(),
    )
    .unwrap();
    let input = prepared.runtime_input().clone();
    let rejected = db
        .accept_runtime_chat_command(input.clone(), host::CHAT, record.id().into(), vec![])
        .unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    assert!(
        db.load_runtime_queue(record.id().into())
            .unwrap()
            .entries()
            .is_empty()
    );
    let retried = db
        .prepare_bus_command(request.clone(), principal.clone())
        .unwrap()
        .prior
        .unwrap();
    assert_eq!(retried.result(), rejected.result());
    assert_eq!(retried.state(), CommandState::Rejected);
    request.conditions.expected_revision = None;
    assert!(matches!(
        db.prepare_bus_command(request, principal),
        Err(Error::CommandConflict)
    ));
}

#[test]
fn catalog_retry_uses_saved_authority_before_resolution_or_conflict() {
    use crate::{
        command_bus::CatalogCommand,
        state::workspace::ProjectId,
        thread::{EnvironmentRef, HostId, ProjectRef},
    };
    let (_dir, mut db) = database();
    let (device, principal) = principal(&mut db);
    let host_id = HostId::new(db.runtime_host().unwrap().id().into()).unwrap();
    let project = ResourceRef::Project(
        ProjectRef::new(host_id.clone(), ProjectId::new(), None, false).unwrap(),
    );
    let cases = [
        (
            ResourceRef::Environment(EnvironmentRef { host_id }),
            CatalogCommand::OpenProject {
                path: "/original/folder".into(),
            },
            CatalogCommand::OpenProject {
                path: "/another/folder".into(),
            },
        ),
        (
            project.clone(),
            CatalogCommand::CloseProject { plan: id() },
            CatalogCommand::CloseProject { plan: id() },
        ),
        (
            project,
            CatalogCommand::CloseTerminal {
                terminal: id(),
                plan: id(),
            },
            CatalogCommand::CloseTerminal {
                terminal: id(),
                plan: id(),
            },
        ),
    ];
    for (resource, command, changed_command) in cases {
        let request = CommandEnvelope {
            command_id: uuid::Uuid::new_v4(),
            resource,
            command: Command::Catalog(command),
            conditions: Conditions::default(),
        };
        let scope = request.command.required_scope();
        assert!(
            db.prepare_bus_catalog(request.clone(), principal.clone(), None)
                .unwrap()
                .is_none()
        );
        let prepared = db
            .prepare_bus_catalog(
                request.clone(),
                principal.clone(),
                Some(("/resolved".into(), "/resolved".into())),
            )
            .unwrap()
            .unwrap();
        let input = prepared.runtime_input().clone();
        db.accept_runtime_command(input.clone(), scope).unwrap();
        db.transition_runtime_command(
            input.key().clone(),
            CommandState::Accepted,
            CommandState::Attempting,
            None,
        )
        .unwrap();
        db.transition_runtime_command(
            input.key().clone(),
            CommandState::Attempting,
            CommandState::DeliveryUnknown,
            None,
        )
        .unwrap();
        let retry = db
            .prepare_bus_catalog(request.clone(), principal.clone(), None)
            .unwrap()
            .unwrap()
            .prior
            .unwrap();
        assert_eq!(retry.state(), CommandState::DeliveryUnknown);
        let mut changed = request.clone();
        changed.command = Command::Catalog(changed_command);
        assert!(matches!(
            db.prepare_bus_catalog(changed.clone(), principal.clone(), None),
            Err(Error::CommandConflict)
        ));
        db.db
            .execute(
                "UPDATE _canopy_remote_devices SET roots='[]' WHERE device_id=?1",
                [&device],
            )
            .unwrap();
        for request in [request, changed] {
            assert!(matches!(
                db.prepare_bus_catalog(request, principal.clone(), None),
                Err(Error::NotAuthorized)
            ));
        }
        db.db
            .execute(
                "UPDATE _canopy_remote_devices SET roots='[\"*\"]' WHERE device_id=?1",
                [&device],
            )
            .unwrap();
    }
}

#[test]
fn typed_fork_keeps_source_identity_and_refuses_another_checkout_or_private() {
    use crate::command_bus::Control;
    let (_dir, mut db) = database();
    let mut source = chat(&mut db);
    let (_, principal) = principal(&mut db);
    let private = envelope(
        &source,
        uuid::Uuid::new_v4(),
        Command::Control(Control::Fork {
            history_epoch: 0,
            item: 2,
            replacement: None,
            local_destination: None,
        }),
    );
    let mut local_private = private.clone();
    local_private.command_id = uuid::Uuid::new_v4();
    if let Command::Control(Control::Fork {
        local_destination, ..
    }) = &mut local_private.command
    {
        *local_destination = Some(crate::command_bus::LocalForkDestination {
            workspace: None,
            chat: uuid::Uuid::new_v4().to_string(),
        });
    }
    assert!(matches!(
        db.prepare_bus_command(local_private.clone(), principal.clone()),
        Err(Error::NotAuthorized)
    ));
    let local = db
        .prepare_bus_command(local_private, Principal::local_owner())
        .unwrap();
    assert!(
        local.prior().is_none(),
        "local Private Fork keeps the existing behavior"
    );
    db.accept_runtime_command(local.input.clone(), host::CHAT)
        .unwrap();
    let rejected = db
        .prepare_bus_command(private.clone(), principal.clone())
        .unwrap()
        .prior
        .unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    assert_eq!(
        db.prepare_bus_command(private, principal.clone())
            .unwrap()
            .prior
            .unwrap(),
        rejected
    );
    source.workspace = Some("source-checkout".into());
    source.authority_root = "/source".into();
    let source = db.save_runtime_chat(source).unwrap();
    let request = envelope(
        &source,
        uuid::Uuid::new_v4(),
        Command::Control(Control::Fork {
            history_epoch: 0,
            item: 2,
            replacement: None,
            local_destination: None,
        }),
    );
    let prepared = db
        .prepare_bus_command(request.clone(), principal.clone())
        .unwrap();
    let input = prepared.runtime_input().clone();
    let key = input.key().clone();
    db.accept_runtime_command(input, host::CHAT).unwrap();
    db.transition_runtime_command(
        key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    let mut fork = chat(&mut db);
    // Completion creates a fresh identity; retained aliases cannot be reused.
    fork.id = id();
    fork.metadata.chat.as_mut().unwrap().id = Some(fork.id.clone());
    fork.thread = None;
    fork.authority_root = "/source".into();
    for destination in [None, Some("different-checkout".into())] {
        fork.workspace = destination;
        assert!(matches!(
            db.complete_runtime_fork(key.clone(), fork.clone(), source.id().into(), None),
            Err(Error::ResourceConflict)
        ));
        assert!(db.load_runtime_chat(fork.id().into()).unwrap().is_none());
    }
    fork.workspace = source.workspace.clone();
    db.complete_runtime_fork(key.clone(), fork.clone(), source.id().into(), None)
        .unwrap();
    let receipt = db
        .prepare_bus_command(request, principal)
        .unwrap()
        .prior
        .unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    assert_eq!(receipt.target(), source.id());
    assert_eq!(receipt.result(), Some(fork.id().as_bytes()));
    assert_eq!(
        db.load_runtime_chat(fork.id().into())
            .unwrap()
            .unwrap()
            .workspace(),
        source.workspace()
    );
}
fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.db");
    crate::settings::database::create(&path).unwrap();
    let mut db = Database::open(&path, Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    (dir, db)
}
fn chat(db: &mut Database) -> ChatRecord {
    let id = id();
    let record = ChatRecord::new(
        id.clone(),
        None,
        "test".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            chat: Some(Box::new(ChatSettings {
                id: Some(id),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    db.save_runtime_chat(record).unwrap()
}
fn principal(db: &mut Database) -> (String, Principal) {
    let device = id();
    db.register_runtime_device(
        DeviceRegistration::new(
            device.clone(),
            "Device".into(),
            device.as_bytes().into(),
            host::ALL_SCOPES,
            vec!["*".into()],
        )
        .unwrap(),
    )
    .unwrap();
    let grant = db.lookup_runtime_device(device.as_bytes().into()).unwrap();
    (device, Principal::authenticated(&grant, 1).unwrap())
}
fn envelope(chat: &ChatRecord, command_id: uuid::Uuid, command: Command) -> CommandEnvelope {
    let thread = chat.thread().unwrap();
    CommandEnvelope {
        command_id,
        resource: ResourceRef::Thread {
            thread: thread.thread().clone(),
            executor_epoch: thread.executor_epoch(),
        },
        command,
        conditions: Conditions::default(),
    }
}
fn rename(chat: &ChatRecord, command_id: uuid::Uuid, title: &str) -> CommandEnvelope {
    envelope(
        chat,
        command_id,
        Command::Rename {
            title: title.into(),
        },
    )
}
fn execute(
    db: &mut Database,
    envelope: CommandEnvelope,
    principal: Principal,
) -> Result<CommandReceipt> {
    let prepared = db.prepare_bus_command(envelope, principal)?;
    if let Some(prior) = prepared.prior {
        return Ok(prior);
    }
    let dispatch = prepared.resolve(None)?;
    db.dispatch_bus_command(prepared, dispatch)
}

#[test]
fn revoke_commits_with_receipt_and_rolls_back_when_admission_is_full() {
    use crate::{
        command_bus::AdminCommand,
        thread::{EnvironmentRef, HostId},
    };
    let (_dir, mut db) = database();
    let (sender, principal) = principal(&mut db);
    let (target, _) = self::principal(&mut db);
    let environment = ResourceRef::Environment(EnvironmentRef {
        host_id: HostId::new(db.runtime_host().unwrap().id().into()).unwrap(),
    });
    let request = CommandEnvelope {
        command_id: uuid::Uuid::new_v4(),
        resource: environment,
        command: Command::Admin(AdminCommand::RevokeDevice {
            device: target.clone(),
        }),
        conditions: Conditions::default(),
    };
    // A full inbox cannot leave a revoke effect without its durable receipt.
    let record = chat(&mut db);
    let fill = execute(
        &mut db,
        rename(&record, uuid::Uuid::new_v4(), "retained"),
        principal.clone(),
    )
    .unwrap();
    db.db
        .execute(
            "UPDATE _canopy_command_inbox SET result=zeroblob(?1) WHERE device_id=?2",
            params![integer(host::REMOTE_INBOX_BYTES).unwrap(), sender],
        )
        .unwrap();
    assert!(matches!(
        execute(&mut db, request.clone(), principal.clone()),
        Err(Error::ResourceLimit)
    ));
    assert!(db.lookup_runtime_device(target.as_bytes().to_vec()).is_ok());
    assert_eq!(
        db.get_runtime_command(fill.key().clone(), 0)
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Succeeded
    );
    // Administrative Close needs no additional row and is idempotent at capacity.
    assert_eq!(
        db.close_runtime_command_epoch(sender.clone(), 1).unwrap(),
        2
    );
    assert_eq!(
        db.close_runtime_command_epoch(sender.clone(), 1).unwrap(),
        2
    );
    let grant = db
        .lookup_runtime_device(sender.as_bytes().to_vec())
        .unwrap();
    let principal = Principal::authenticated(&grant, 2).unwrap();
    let receipt = execute(&mut db, request.clone(), principal.clone()).unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    assert!(matches!(
        db.lookup_runtime_device(target.as_bytes().to_vec()),
        Err(Error::NotAuthorized)
    ));
    let revision: i64 = db
        .db
        .query_row(
            "SELECT authorization_revision FROM _canopy_remote_devices WHERE device_id=?1",
            [&target],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        execute(&mut db, request.clone(), principal.clone()).unwrap(),
        receipt
    );
    assert_eq!(
        db.db
            .query_row::<i64, _, _>(
                "SELECT authorization_revision FROM _canopy_remote_devices WHERE device_id=?1",
                [&target],
                |r| r.get(0)
            )
            .unwrap(),
        revision
    );
    let mut changed = request;
    changed.command = Command::Admin(AdminCommand::RevokeDevice { device: id() });
    assert!(matches!(
        execute(&mut db, changed.clone(), principal.clone()),
        Err(Error::CommandConflict)
    ));
    db.update_runtime_device_authorization(
        sender.clone(),
        grant.authorization_revision(),
        host::READ,
        vec!["private".into()],
    )
    .unwrap();
    assert!(matches!(
        execute(&mut db, changed, principal),
        Err(Error::NotAuthorized)
    ));
    let mut wrong_epoch = receipt.key().clone();
    wrong_epoch.epoch = 1;
    assert!(matches!(
        db.get_runtime_command(wrong_epoch, 0),
        Err(Error::NotAuthorized)
    ));
    // Local recovery remains available even when the local inbox cannot admit.
    let local = execute(
        &mut db,
        rename(&record, uuid::Uuid::new_v4(), "local"),
        Principal::local_owner(),
    )
    .unwrap();
    db.db
        .execute(
            "UPDATE _canopy_command_inbox SET result=zeroblob(?1) WHERE device_id='local'",
            [integer(host::LOCAL_INBOX_BYTES).unwrap()],
        )
        .unwrap();
    db.revoke_runtime_device(sender.clone()).unwrap();
    assert!(matches!(
        db.lookup_runtime_device(sender.as_bytes().to_vec()),
        Err(Error::NotAuthorized)
    ));
    db.acknowledge_runtime_command(local.key().clone()).unwrap();
    assert!(
        db.get_runtime_command(local.key().clone(), 0)
            .unwrap()
            .is_none()
    );
}
fn send(chat: &ChatRecord, message: String) -> Command {
    Command::Send {
        message,
        draft: incoming("original"),
        context: chat.send_context(),
        steer_run: None,
    }
}
fn incoming(text: &str) -> DraftIntent {
    DraftIntent::Incoming {
        text: text.into(),
        context: None,
        goal: false,
        attachments: vec![],
    }
}
fn preferences(
    db: &Database,
    command_id: uuid::Uuid,
    value: bool,
    revision: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id,
        resource: ResourceRef::Environment(crate::thread::EnvironmentRef {
            host_id: HostId::new(db.runtime_host().unwrap().id).unwrap(),
        }),
        command: Command::preferences(vec![crate::settings::Change::NotchEnabled(value)]).unwrap(),
        conditions: Conditions {
            expected_revision: revision,
            ..Default::default()
        },
    }
}

#[test]
fn bus_retry_reopens_retains_deleted_target_and_normalizes_before_fingerprinting() {
    let (dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let original = rename(&chat, command_id, "  Committed name  ");
    let receipt = execute(&mut db, original.clone(), principal.clone()).unwrap();
    let saved = db.load_runtime_chat(chat.id.clone()).unwrap().unwrap();
    assert_eq!(saved.metadata.title.as_deref(), Some("Committed name"));
    assert!(saved.thread().unwrap().revision() > chat.thread().unwrap().revision());
    // A delayed normal runtime save cannot undo a bus-owned explicit name.
    let late = db.save_runtime_chat(chat.clone()).unwrap();
    assert_eq!(late.metadata.title.as_deref(), Some("Committed name"));
    assert_eq!(
        execute(
            &mut db,
            rename(&chat, command_id, "Committed name"),
            principal.clone()
        )
        .unwrap(),
        receipt
    );
    let other_method = envelope(
        &chat,
        command_id,
        Command::Queue {
            revision: 0,
            change: QueueChange::Pause { paused: false },
        },
    );
    assert!(matches!(
        execute(&mut db, other_method, principal.clone()),
        Err(Error::CommandConflict)
    ));
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        receipt
    );
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [chat.id()],
        )
        .unwrap();
    let alias = db
        .runtime_thread_alias(
            chat.thread().unwrap().thread().host_id.clone(),
            chat.id.clone(),
        )
        .unwrap();
    assert_eq!(alias, chat.thread_id());
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        receipt
    );
    let mut changed_conditions = original;
    changed_conditions.conditions.expected_revision = Some(1);
    assert!(matches!(
        execute(&mut db, changed_conditions, principal),
        Err(Error::CommandConflict)
    ));
}

#[test]
fn current_authority_precedes_dedupe_and_conflict_and_dispatch_rechecks_it() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (device, principal) = principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let original = rename(&chat, command_id, "original");
    execute(&mut db, original.clone(), principal.clone()).unwrap();
    let pending = db
        .prepare_bus_command(
            rename(&chat, uuid::Uuid::new_v4(), "must not run"),
            principal.clone(),
        )
        .unwrap();
    let dispatch = pending.resolve(None).unwrap();
    db.update_runtime_device_authorization(device.clone(), 1, host::READ, vec!["*".into()])
        .unwrap();
    for intent in [original.clone(), rename(&chat, command_id, "different")] {
        assert!(matches!(
            execute(&mut db, intent, principal.clone()),
            Err(Error::NotAuthorized)
        ));
    }
    assert!(matches!(
        db.dispatch_bus_command(pending, dispatch),
        Err(Error::NotAuthorized)
    ));
    assert_eq!(
        db.load_runtime_chat(chat.id.clone())
            .unwrap()
            .unwrap()
            .metadata
            .title
            .as_deref(),
        Some("original")
    );
    db.revoke_runtime_device(device).unwrap();
    assert!(matches!(
        execute(&mut db, original, principal),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn composite_identity_isolates_threads_executors_and_principals_and_legacy_is_ambiguous() {
    let (_dir, mut db) = database();
    let first = chat(&mut db);
    let second = chat(&mut db);
    let (device, principal) = principal(&mut db);
    let (_other_device, other) = self::principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let a = execute(
        &mut db,
        rename(&first, command_id, "first"),
        principal.clone(),
    )
    .unwrap();
    let b = execute(
        &mut db,
        rename(&second, command_id, "second"),
        principal.clone(),
    )
    .unwrap();
    let c = execute(
        &mut db,
        rename(&first, command_id, "other principal"),
        other,
    )
    .unwrap();
    db.db
        .execute(
            "UPDATE _canopy_runtime_chats SET executor_epoch=1 WHERE chat_id=?1",
            [first.id()],
        )
        .unwrap();
    let current = db.load_runtime_chat(first.id.clone()).unwrap().unwrap();
    let d = execute(
        &mut db,
        rename(&current, command_id, "next executor"),
        principal.clone(),
    )
    .unwrap();
    assert_ne!(a.key(), b.key());
    assert_ne!(a.key(), c.key());
    assert_ne!(a.key(), d.key());
    assert_eq!(
        execute(
            &mut db,
            rename(&first, command_id, "first"),
            principal.clone(),
        )
        .unwrap(),
        a
    );
    assert!(matches!(
        execute(
            &mut db,
            rename(&first, command_id, "changed old intent"),
            principal.clone(),
        ),
        Err(Error::CommandConflict)
    ));
    assert_eq!(
        db.get_runtime_command(a.key.clone(), 0).unwrap(),
        Some(a.clone())
    );
    let legacy = CommandKey::remote(device.clone(), 1, command_id.to_string()).unwrap();
    assert!(matches!(
        db.get_runtime_command(legacy.clone(), 0),
        Err(Error::AmbiguousCommand)
    ));
    assert!(matches!(
        db.resolve_runtime_command(legacy),
        Err(Error::AmbiguousCommand)
    ));
    // State transitions and resolution affect exactly one full key.
    db.db.execute("UPDATE _canopy_command_inbox SET state='accepted' WHERE resource=?1 AND executor_epoch=?2 AND device_id=?3 AND command_id=?4",params![d.key.resource,1,device,command_id.to_string()]).unwrap();
    db.transition_runtime_command(
        d.key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    db.transition_runtime_command(
        d.key.clone(),
        CommandState::Attempting,
        CommandState::DeliveryUnknown,
        None,
    )
    .unwrap();
    db.resolve_runtime_command(d.key.clone()).unwrap();
    assert_eq!(db.get_runtime_command(b.key.clone(), 0).unwrap(), Some(b));
    assert_eq!(db.get_runtime_command(a.key.clone(), 0).unwrap(), Some(a));
    assert!(
        db.get_runtime_command(d.key, 0)
            .unwrap()
            .unwrap()
            .resolved_by_user()
    );
    assert_eq!(
        execute(
            &mut db,
            rename(&first, uuid::Uuid::new_v4(), "stale executor"),
            principal
        )
        .unwrap()
        .state(),
        CommandState::Rejected
    );
}

#[test]
fn stale_executor_rejection_is_durable_and_authorized_before_retry_or_conflict() {
    let (dir, mut db) = database();
    let chat = chat(&mut db);
    let (device, principal) = principal(&mut db);
    db.db
        .execute(
            "UPDATE _canopy_runtime_chats SET executor_epoch=1 WHERE chat_id=?1",
            [chat.id()],
        )
        .unwrap();
    let id = uuid::Uuid::new_v4();
    let original = rename(&chat, id, "must not change");
    let rejected = execute(&mut db, original.clone(), principal.clone()).unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    assert_eq!(rejected.key().executor_epoch(), 0);
    assert_eq!(
        db.load_runtime_chat(chat.id.clone())
            .unwrap()
            .unwrap()
            .metadata
            .title,
        chat.metadata.title
    );
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        rejected
    );
    assert!(matches!(
        execute(&mut db, rename(&chat, id, "changed"), principal.clone()),
        Err(Error::CommandConflict)
    ));
    db.revoke_runtime_device(device).unwrap();
    for intent in [original, rename(&chat, id, "changed")] {
        assert!(matches!(
            execute(&mut db, intent, principal.clone()),
            Err(Error::NotAuthorized)
        ));
    }
}

#[test]
fn local_acknowledgement_and_authorization_updates_use_the_full_key() {
    let (_dir, mut db) = database();
    let first = chat(&mut db);
    let mut second = chat(&mut db);
    second.workspace = Some("project".into());
    second.authority_root = "/project".into();
    second = db.save_runtime_chat(second).unwrap();
    let command_id = uuid::Uuid::new_v4();
    let a = execute(
        &mut db,
        rename(&first, command_id, "first"),
        Principal::local_owner(),
    )
    .unwrap();
    let b = execute(
        &mut db,
        rename(&second, command_id, "second"),
        Principal::local_owner(),
    )
    .unwrap();
    db.acknowledge_runtime_command(a.key.clone()).unwrap();
    assert!(receipt(&db.db, &a.key).unwrap().is_none());
    assert_eq!(receipt(&db.db, &b.key).unwrap(), Some(b));
    let (device, principal) = principal(&mut db);
    let a = execute(&mut db, rename(&first, command_id, "a"), principal.clone()).unwrap();
    let b = execute(&mut db, rename(&second, command_id, "b"), principal).unwrap();
    db.db
        .execute(
            "UPDATE _canopy_command_inbox SET state='accepted' WHERE device_id=?1",
            [&device],
        )
        .unwrap();
    db.update_runtime_device_authorization(device, 1, host::ALL_SCOPES, vec!["private".into()])
        .unwrap();
    assert_eq!(
        receipt(&db.db, &a.key).unwrap().unwrap().state(),
        CommandState::Accepted
    );
    assert_eq!(
        receipt(&db.db, &b.key).unwrap().unwrap().state(),
        CommandState::Rejected
    );
}

#[test]
fn failed_receipt_commit_rolls_back_rename_send_queue_and_preferences() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let old_queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    db.db.execute_batch("CREATE TRIGGER fail_bus BEFORE INSERT ON _canopy_command_inbox BEGIN SELECT RAISE(ABORT,'receipt failure'); END;").unwrap();
    for command in [
        Command::Rename {
            title: "not committed".into(),
        },
        send(&chat, id()),
        Command::Queue {
            revision: old_queue.revision(),
            change: QueueChange::Pause { paused: false },
        },
    ] {
        assert!(
            execute(
                &mut db,
                envelope(&chat, uuid::Uuid::new_v4(), command),
                Principal::local_owner()
            )
            .is_err()
        );
    }
    assert_eq!(
        db.load_runtime_chat(chat.id.clone()).unwrap(),
        Some(chat.clone())
    );
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), old_queue);
    let prefs = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db
        .prepare_bus_command(prefs, Principal::local_owner())
        .unwrap();
    assert!(db.commit_bus_preferences(prepared, 42).is_err());
    assert!(db.load().unwrap().preferences.notch_enabled);
    assert_eq!(
        db.db
            .query_row("SELECT count(*) FROM _canopy_command_inbox", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn preferences_retry_returns_original_committed_values_and_revision_after_later_writes() {
    let (dir, mut db) = database();
    let (_device, principal) = principal(&mut db);
    let original = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let (first, snapshot) = db.commit_bus_preferences(prepared, 42).unwrap();
    assert!(!snapshot.unwrap().preferences.notch_enabled);
    let result: serde_json::Value = serde_json::from_slice(first.result().unwrap()).unwrap();
    assert_eq!(result["values"]["notch_enabled"], false);
    assert_eq!(result["revision"], 43);
    let later = preferences(&db, uuid::Uuid::new_v4(), true, Some(43));
    let prepared = db.prepare_bus_command(later, principal.clone()).unwrap();
    let (second, _) = db.commit_bus_preferences(prepared, 43).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(second.result().unwrap()).unwrap()["revision"],
        44
    );
    let stale = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db.prepare_bus_command(stale, principal.clone()).unwrap();
    let (rejected, snapshot) = db.commit_bus_preferences(prepared, 44).unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    assert!(snapshot.is_none());
    assert!(db.load().unwrap().preferences.notch_enabled);
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let (retry, snapshot) = db.commit_bus_preferences(prepared, 1000).unwrap();
    assert_eq!(retry, first);
    assert!(snapshot.is_none());
    let mut changed = original;
    changed.conditions.expected_revision = Some(43);
    assert!(matches!(
        db.prepare_bus_command(changed, principal),
        Err(Error::CommandConflict)
    ));
    assert!(db.load().unwrap().preferences.notch_enabled);
}

#[test]
fn send_dedupe_precedes_context_and_deleted_target_and_pause_is_not_undone() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let message = id();
    let original = envelope(&chat, uuid::Uuid::new_v4(), send(&chat, message.clone()));
    let first = execute(&mut db, original.clone(), principal.clone()).unwrap();
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert!(!queue.paused());
    assert_eq!(queue.entries().len(), 1);
    let pause = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: queue.revision(),
            change: QueueChange::Pause { paused: true },
        },
    );
    execute(&mut db, pause, principal.clone()).unwrap();
    let paused = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        first
    );
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), paused);
    let mut changed = original.clone();
    if let Command::Send { context, .. } = &mut changed.command {
        context.history_epoch += 1;
    }
    assert!(matches!(
        execute(&mut db, changed, principal.clone()),
        Err(Error::CommandConflict)
    ));
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [chat.id()],
        )
        .unwrap();
    assert_eq!(execute(&mut db, original, principal).unwrap(), first);
}

#[test]
fn incoming_retry_precedes_vanished_workspace_and_rotated_context_and_checks_read_scope() {
    use crate::command_bus::{AttachmentIntent, DraftResources, PullIntent};
    let (_dir, mut db) = database();
    let mut chat = chat(&mut db);
    chat.workspace = Some("project".into());
    chat.authority_root = "/project".into();
    chat = db.save_runtime_chat(chat).unwrap();
    let (device, principal) = principal(&mut db);
    let resources = DraftResources {
        roots: Default::default(),
        pulls: Default::default(),
    };
    resources
        .roots
        .replace([("project".into(), "/project".into())].into())
        .unwrap();
    let pull = resources.pulls.publish(
        "/project",
        crate::api::proto::PullAttachment {
            number: 42,
            title: "context".into(),
            ..Default::default()
        },
    );
    let original = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Send {
            message: id(),
            context: chat.send_context(),
            steer_run: None,
            draft: DraftIntent::Incoming {
                text: "with references".into(),
                context: None,
                goal: false,
                attachments: vec![
                    AttachmentIntent::ThreadPath {
                        project_file: true,
                        relative: "src/file.rs".into(),
                        label: None,
                    },
                    AttachmentIntent::Pull {
                        label: None,
                        value: PullIntent {
                            number: pull.number,
                            title: pull.title,
                            url: pull.url,
                            head: pull.head,
                            base: pull.base,
                            state: pull.state,
                            body: pull.body,
                            context_id: pull.context_id,
                        },
                    },
                ],
            },
        },
    );
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let dispatch = prepared.resolve(Some(&resources)).unwrap();
    let receipt = db.dispatch_bus_command(prepared, dispatch).unwrap();
    // A fresh request still resolves its references; a retry never needs them again.
    resources.roots.replace(Default::default()).unwrap();
    let unavailable = DraftResources {
        roots: resources.roots,
        pulls: Default::default(),
    };
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    assert_eq!(prepared.prior(), Some(&receipt));
    let mut new_request = original.clone();
    new_request.command_id = uuid::Uuid::new_v4();
    let prepared = db
        .prepare_bus_command(new_request.clone(), principal.clone())
        .unwrap();
    assert!(prepared.resolve(Some(&unavailable)).is_err());
    unavailable
        .roots
        .replace([("project".into(), "/project".into())].into())
        .unwrap();
    assert!(prepared.resolve(Some(&unavailable)).is_err());
    assert_eq!(
        db.load_runtime_queue(chat.id.clone())
            .unwrap()
            .entries()
            .len(),
        1
    );
    db.update_runtime_device_authorization(device, 1, host::CHAT, vec!["*".into()])
        .unwrap();
    assert!(matches!(
        db.prepare_bus_command(original, principal.clone()),
        Err(Error::NotAuthorized)
    ));
    assert!(matches!(
        db.get_runtime_command(receipt.key.clone(), 0),
        Err(Error::NotAuthorized)
    ));
    assert!(matches!(
        db.prepare_bus_command(new_request, principal),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn every_queue_change_is_durable_deduped_and_revision_checked() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let first = id();
    let second = id();
    let resent = id();
    let before = db.load_runtime_queue(chat.id.clone()).unwrap();
    let invalid = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: before.revision(),
            change: QueueChange::Remove { message: id() },
        },
    );
    let rejected = execute(&mut db, invalid.clone(), principal.clone()).unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    // Validation failed after its tentative queue revision increment.
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), before);
    assert_eq!(
        execute(&mut db, invalid, principal.clone()).unwrap(),
        rejected
    );
    for message in [&first, &second] {
        execute(
            &mut db,
            envelope(&chat, uuid::Uuid::new_v4(), send(&chat, message.clone())),
            principal.clone(),
        )
        .unwrap();
    }
    let changes = [
        QueueChange::Edit {
            message: first.clone(),
            draft: incoming("edited"),
        },
        QueueChange::Move {
            message: second.clone(),
            before: Some(first.clone()),
        },
        QueueChange::Pause { paused: true },
        QueueChange::Remove {
            message: second.clone(),
        },
    ];
    for change in changes {
        let before = db.load_runtime_queue(chat.id.clone()).unwrap();
        let request = envelope(
            &chat,
            uuid::Uuid::new_v4(),
            Command::Queue {
                revision: before.revision(),
                change,
            },
        );
        let receipt = execute(&mut db, request.clone(), principal.clone()).unwrap();
        let after = db.load_runtime_queue(chat.id.clone()).unwrap();
        assert_eq!(after.revision(), before.revision() + 1);
        assert_eq!(
            execute(&mut db, request.clone(), principal.clone()).unwrap(),
            receipt
        );
        assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), after);
        let mut stale = request;
        stale.command_id = uuid::Uuid::new_v4();
        let rejected = execute(&mut db, stale.clone(), principal.clone()).unwrap();
        assert_eq!(rejected.state(), CommandState::Rejected);
        assert_eq!(
            execute(&mut db, stale, principal.clone()).unwrap(),
            rejected
        );
        assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), after);
    }
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert!(queue.paused());
    assert_eq!(queue.entries()[0].id(), second);
    assert_eq!(queue.entries()[0].state(), host::QueueState::Dismissed);
    assert_eq!(queue.entries()[1].draft().text(), "edited");
    // The existing delivery owner fences attempting; restart creates uncertainty.
    let queue = db
        .set_runtime_queue_paused(chat.id.clone(), queue.revision(), false)
        .unwrap();
    let queue = db
        .transition_runtime_queue(
            chat.id.clone(),
            first.clone(),
            queue.revision(),
            host::QueueState::Queued,
            host::QueueState::Attempting,
        )
        .unwrap();
    db.transition_runtime_queue(
        chat.id.clone(),
        first.clone(),
        queue.revision(),
        host::QueueState::Attempting,
        host::QueueState::DeliveryUnknown,
    )
    .unwrap();
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    let request = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: queue.revision(),
            change: QueueChange::ResolveUnknown {
                message: first,
                resolution: UnknownResolution::Resend {
                    message: resent.clone(),
                },
            },
        },
    );
    let receipt = execute(&mut db, request.clone(), principal.clone()).unwrap();
    assert_eq!(execute(&mut db, request, principal).unwrap(), receipt);
    let queue = db.load_runtime_queue(chat.id).unwrap();
    assert_eq!(
        queue
            .entries()
            .iter()
            .filter(|entry| entry.id() == resent)
            .count(),
        1
    );
    assert_eq!(
        queue
            .entries()
            .iter()
            .filter(|entry| entry.state() == host::QueueState::Queued)
            .count(),
        1
    );
}

#[test]
fn wrong_host_target_and_conditions_never_accept_an_effect() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let mut wrong_host = rename(&chat, uuid::Uuid::new_v4(), "wrong host");
    if let ResourceRef::Thread { thread, .. } = &mut wrong_host.resource {
        thread.host_id = HostId::new(id()).unwrap();
    }
    assert!(matches!(
        execute(&mut db, wrong_host, Principal::local_owner()),
        Err(Error::NotFound)
    ));
    let mut stale = rename(&chat, uuid::Uuid::new_v4(), "stale");
    stale.conditions.expected_revision = Some(0);
    assert_eq!(
        execute(&mut db, stale, Principal::local_owner())
            .unwrap()
            .state(),
        CommandState::Rejected
    );
    assert_eq!(db.load_runtime_chat(chat.id.clone()).unwrap(), Some(chat));
}
