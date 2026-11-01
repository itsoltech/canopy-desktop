//! Closing a project whose chats' cleanup fails: the command fails (no false
//! removal), the project stays listed as closing, and closing it again on the same
//! workspace retries the cleanup until it succeeds.
use super::{AppState, catalog, close_preflight, close_project, listed};
use crate::app_state::chats::RemoteKey;
use canopy_desktop::{
    chat::config::ChatSettings,
    command_bus::{CatalogCommand, Command, CommandBus, CommandEnvelope, Principal, ResourceRef},
    host::{ChatRecord, CommandKey, CommandReceipt, CommandState},
    settings::SettingsClient,
    state::workspace::{PaneKind, PaneMetadata, Workspace, WorkspaceId},
    thread::{HostId, ProjectRef, WorktreeId},
};
use gpui_kit::TestAppContext;

async fn close(cx: &mut TestAppContext, workspace: WorkspaceId, key: CommandKey) -> CommandReceipt {
    let preflight = cx
        .update(|cx| close_preflight(workspace, cx))
        .expect("the project can be closed");
    let (client, (_, project, _, path, repository, _)) = cx.update(|cx| {
        let app = cx.global::<AppState>();
        (
            app.settings.read(cx).client().unwrap(),
            listed(cx)
                .into_iter()
                .find(|(id, ..)| *id == workspace)
                .unwrap(),
        )
    });
    let host = client.initialize_runtime().await.unwrap();
    let root = path.canonicalize().unwrap().to_str().unwrap().to_owned();
    let request = CommandEnvelope {
        command_id: uuid::Uuid::parse_str(key.id()).unwrap(),
        resource: ResourceRef::Project(
            ProjectRef::new(
                HostId::new(host.id().into()).unwrap(),
                project,
                repository
                    .as_ref()
                    .map(|_| WorktreeId::new(serde_json::to_string(&workspace).unwrap()).unwrap()),
                repository.is_some(),
            )
            .unwrap(),
        ),
        command: Command::Catalog(CatalogCommand::CloseProject {
            plan: uuid::Uuid::new_v4().to_string(),
        }),
        conditions: Default::default(),
    };
    let bus = CommandBus::new(client);
    let prepared = bus
        .prepare_catalog(
            request.clone(),
            Principal::local_owner(),
            Some((root.clone(), root)),
        )
        .await
        .unwrap()
        .unwrap();
    let task = cx.update(|cx| {
        close_project(
            RemoteKey::admitted(prepared.runtime_input().clone()),
            preflight,
            cx,
        )
    });
    cx.run_until_parked();
    let mut task = Box::pin(task);
    for _ in 0..500 {
        cx.run_until_parked();
        if let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task))
        {
            let receipt = result.expect("the close is recorded");
            let retry = bus
                .prepare_catalog(request, Principal::local_owner(), None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                retry.prior(),
                Some(&receipt),
                "retry after catalog removal returns the retained outcome"
            );
            return receipt;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the close didn't finish");
}

#[gpui_kit::test]
async fn a_close_whose_cleanup_failed_stays_listed_and_closing_again_retries_it(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let folder = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&folder.path().join("settings.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let checkout = folder.path().join("project");
    std::fs::create_dir(&checkout).unwrap();
    let checkout = std::fs::canonicalize(checkout).unwrap();
    let (app, workspace) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings.update(cx, |settings, _| {
            settings.install_test_client(client.clone(), host)
        });
        app.projects.update(cx, |projects, _| {
            projects.install_test_client(client.clone())
        });
        let workspace = app
            .projects
            .update(cx, |projects, cx| {
                projects.open_without_selecting(checkout.clone(), cx)
            })
            .unwrap();
        app.chats.update(cx, |chats, cx| chats.sync_model(cx));
        (app, workspace)
    });
    // A chat of the project without a tab (created remotely).
    let chat = ChatSettings::new_id();
    let record = ChatRecord::new(
        chat.clone(),
        Some(serde_json::to_string(&workspace).unwrap()),
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            cwd: Some(checkout.clone()),
            chat: Some(Box::new(ChatSettings {
                id: Some(chat.clone()),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .and_then(|r| r.with_authority_root(checkout.to_str().unwrap().into()))
    .unwrap();
    let record = client.save_runtime_chat(record).await.unwrap();
    cx.update(|cx| {
        app.settings
            .update(cx, |settings, _| settings.record_saved(record));
        app.chats.update(cx, |chats, cx| chats.sync_model(cx));
        assert_eq!(
            close_preflight(workspace, cx).unwrap().chats,
            vec![chat.clone()]
        );
    });

    // Chat history isn't loaded yet: the chat's cleanup fails.
    let first = CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap();
    let failed = close(cx, workspace, first.clone()).await;
    assert_eq!(failed.state(), CommandState::Failed);
    cx.update(|cx| {
        assert!(
            !app.projects
                .read(cx)
                .catalog
                .items
                .iter()
                .any(|p| p.workspace == workspace)
        );
        // Before: the project vanished and nothing remote could retry its cleanup.
        let listed = listed(cx);
        assert!(
            listed
                .iter()
                .any(|(id, .., closing)| *id == workspace && *closing)
        );
        assert!(
            !catalog(cx).iter().any(|(id, ..)| *id == workspace),
            "terminals and new work never see a closing project"
        );
        let retry = close_preflight(workspace, cx).unwrap();
        assert_eq!(retry.chats, vec![chat.clone()]);
        assert!(retry.panes.is_empty());
    });
    // The original command keeps its outcome.
    assert_eq!(
        client
            .get_runtime_command(failed.key().clone(), canopy_desktop::host::CHAT)
            .await
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Failed
    );

    // Once history is available, closing it again retries the cleanup only.
    cx.update(|cx| {
        app.chat_history
            .update(cx, |history, cx| history.initialize(client.clone(), cx))
    });
    let second = CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap();
    let done = close(cx, workspace, second).await;
    assert_eq!(done.state(), CommandState::Succeeded);
    cx.update(|cx| {
        assert!(!listed(cx).iter().any(|(id, ..)| *id == workspace));
        assert_eq!(
            close_preflight(workspace, cx).unwrap_err(),
            "resource_deleted"
        );
        assert!(app.chats.read(cx).unsettled_cleanup(workspace).is_empty());
    });
    client.shutdown().await.unwrap();
}
