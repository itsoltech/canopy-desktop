use super::{ClientGlobal, Page, RemoteWindow, ensure_global};
use canopy_desktop::remote::client::{
    ClientRegistry, Phase,
    store::{CredentialStore, HostRecord},
};
use gpui_kit::{
    AppContext, TestAppContext,
    component::Root,
    px, size,
    test::{TestAppContextExt, TestWindowExt},
};
use std::{sync::Arc, time::Duration};

struct NoCredentials;
impl CredentialStore for NoCredentials {
    fn load(&self, _: &str) -> Result<String, String> {
        panic!("Opening Connections must not connect or read credentials")
    }
    fn save(&self, _: &str, _: &str) -> Result<(), String> {
        panic!("No enrollment in this test")
    }
    fn remove(&self, _: &str) -> Result<(), String> {
        panic!("No credential removal in this test")
    }
}

#[gpui_kit::test]
async fn adding_a_route_uses_the_shared_registry_without_another_pairing(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clients.json");
    let registry = ClientRegistry::open(path.clone(), Arc::new(NoCredentials)).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let record: HostRecord = serde_json::from_value(serde_json::json!({
        "id":id,"name":"Mac Studio","endpoint":"https://192.168.1.30:44330",
        "server_name":"paired-host","certificate":[],"fingerprint":[],"device_id":"device",
        "credential_ref":"never-read","ssh":null,"last_seen":null,"epoch":null,"closing_epoch":null,
        "catalog":[],"outbox":[],"mutations":[],"views":{},"uploads":[],"chats":{}
    }))
    .unwrap();
    let host = registry.insert(record).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(cx);
        ensure_global(cx).unwrap();
        cx.global::<ClientGlobal>()
            .registry
            .set(registry.clone())
            .unwrap_or_else(|_| panic!("new registry"));
    });
    let mut view = None;
    let handle = cx.open_window(size(px(1100.), px(1800.)), |window, cx| {
        let entity = cx.new(|cx| {
            let mut view = RemoteWindow::new(window, cx);
            view.opening_setup = true;
            view
        });
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        !view.read(cx).busy
    })
    .await;
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).host.is_none(),
            "Add computer must not restore the last saved host over its form"
        );
        assert_eq!(host.phase(), Phase::Offline);
        assert!(host.connection_error().is_none());
        window.click("pair-host", cx);
        assert!(
            !view.read(cx).busy,
            "An empty invitation must not start enrollment"
        );
        assert!(view.read(cx).error.is_none());
        assert!(window.try_find("probe-ssh").is_none());
        window.click("pair-ssh-mode", cx);
        assert!(window.find("probe-ssh").visible());
        window.click("pair-direct-mode", cx);
        assert!(window.try_find("probe-ssh").is_none());
        window.click(id.clone(), cx);
        window.click("add-host", cx);
        assert!(matches!(view.read(cx).page, Page::Hosts));
        window.click("new-connection-route", cx);
        window.click("route-name-input", cx);
        window.input("Office", cx);
        window.click("route-endpoint-input", cx);
        window.input("192.168.128.30:44330", cx);
        window.click("save-connection-route", cx);
    })
    .unwrap();
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        !view.read(cx).busy
    })
    .await;
    cx.update(|cx| {
        let connections = cx.global::<ClientGlobal>().connections.read(cx);
        let shared = connections
            .hosts()
            .into_iter()
            .find(|h| h.id() == host.id())
            .unwrap();
        assert!(Arc::ptr_eq(&shared, &host));
        assert_eq!(shared.routes().len(), 2);
        assert_eq!(shared.routes()[1].name(), "Office");
        assert_eq!(shared.phase(), Phase::Offline);
        assert!(view.read(cx).error.is_none(), "{:?}", view.read(cx).error);
    });
    let restored = ClientRegistry::open(path, Arc::new(NoCredentials))
        .unwrap()
        .get(host.id())
        .unwrap();
    assert_eq!(
        restored.routes()[1].endpoint(),
        "https://192.168.128.30:44330/"
    );
}

#[gpui_kit::test]
async fn settings_edit_routes_inline_without_connecting_or_opening_a_window(
    cx: &mut TestAppContext,
) {
    use super::{ConnectionSetup, ConnectionSetupEvent};
    use std::{cell::Cell, rc::Rc};
    cx.background_executor.allow_parking();
    let directory = tempfile::tempdir().unwrap();
    let registry = ClientRegistry::open(
        directory.path().join("clients.json"),
        Arc::new(NoCredentials),
    )
    .unwrap();
    let record: HostRecord = serde_json::from_value(serde_json::json!({
        "id":uuid::Uuid::new_v4().to_string(),"name":"Mac Studio",
        "endpoint":"https://192.168.1.30:44330","server_name":"paired-host","certificate":[],
        "fingerprint":[],"device_id":"device","credential_ref":"never-read","ssh":null,
        "last_seen":null,"epoch":null,"closing_epoch":null,"catalog":[],"outbox":[],
        "mutations":[],"views":{},"uploads":[],"chats":{}
    }))
    .unwrap();
    let host = registry.insert(record).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(cx);
        ensure_global(cx).unwrap();
        cx.global::<ClientGlobal>()
            .registry
            .set(registry.clone())
            .unwrap_or_else(|_| panic!("new registry"));
    });
    let mut setup = None;
    let routes_host = host.clone();
    let handle = cx.open_window(size(px(720.), px(1400.)), |window, cx| {
        let entity = ConnectionSetup::routes(routes_host, window, cx).unwrap();
        setup = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let setup = setup.unwrap();
    let closed = Rc::new(Cell::new(false));
    let _events = cx.update(|cx| {
        let closed = closed.clone();
        cx.subscribe(&setup, move |_, event: &ConnectionSetupEvent, _| {
            closed.set(matches!(event, ConnectionSetupEvent::Closed))
        })
    });
    let view = cx.update(|cx| setup.read(cx).view.clone());
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        !view.read(cx).busy
    })
    .await;
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            cx.windows().len(),
            1,
            "Routes must not open the device window"
        );
        window.click("new-connection-route", cx);
        window.click("route-name-input", cx);
        window.input("Office", cx);
        window.click("route-endpoint-input", cx);
        window.input("192.168.128.30:44330", cx);
        window.click("save-connection-route", cx);
    })
    .unwrap();
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        !view.read(cx).busy
    })
    .await;
    cx.update_window(handle.into(), |_, window, cx| {
        assert!(view.read(cx).error.is_none(), "{:?}", view.read(cx).error);
        assert_eq!(host.routes().len(), 2);
        assert_eq!(host.routes()[1].name(), "Office");
        assert_eq!(
            host.phase(),
            Phase::Offline,
            "Editing routes must not connect"
        );
        window.render_frame(cx);
        window.click("close-computer-routes", cx);
    })
    .unwrap();
    assert!(closed.get(), "Done closes the inline routes");
}

#[gpui_kit::test]
async fn the_sidebar_groups_origins_but_keeps_each_computers_main_and_chats(
    cx: &mut TestAppContext,
) {
    use crate::{
        app_state::AppState,
        ui::sidebar::{Navigation, Sidebar},
    };
    use canopy_desktop::{
        git::{RepositoryIdentity, RepositoryInfo, WorktreeHead, WorktreeInfo},
        remote::proto as p,
        state::workspace::{PaneKind, PaneMetadata, Workspace},
    };
    use gpui_kit::{Entity, Subscription};
    use prost::Message;
    use std::{cell::RefCell, rc::Rc};
    cx.background_executor.allow_parking();
    let directory = tempfile::tempdir().unwrap();
    let registry = ClientRegistry::open(
        directory.path().join("clients.json"),
        Arc::new(NoCredentials),
    )
    .unwrap();
    let mut hosts = vec![];
    for (name, origin) in [
        ("Home", "https://github.com/itsoltech/canopy-desktop"),
        (
            "Office",
            "ssh://git@github.com/itsoltech/canopy-desktop.git",
        ),
        ("Other forge", "https://gitlab.com/itsoltech/canopy-desktop"),
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        let workspace = p::WorkspaceRef {
            host_id: id.clone(),
            workspace_id: "main".into(),
        };
        let catalog = p::Catalog {
            host_id: id.clone(),
            projects: vec![p::ProjectSummary {
                project_id: "/different/path".into(),
                name: "canopy-desktop".into(),
                path: "/different/path".into(),
                origin: Some(origin.into()),
                checkouts: vec![p::WorkspaceSummary {
                    workspace: Some(workspace.clone()),
                    name: "checkout".into(),
                    branch: Some("main".into()),
                    ..Default::default()
                }],
            }],
            chats: vec![p::ChatSummary {
                chat: Some(p::ChatRef {
                    host_id: id.clone(),
                    chat_id: "same-chat-id".into(),
                }),
                workspace: Some(workspace),
                title: "Plan".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let record: HostRecord = serde_json::from_value(serde_json::json!({
            "id":id,"name":name,"endpoint":"https://192.168.1.30:44330", "server_name":"paired-host","certificate":[],"fingerprint":[],"device_id":"device","credential_ref":"never-read","ssh":null,"last_seen":null,"epoch":null,"closing_epoch":null,"catalog":catalog.encode_to_vec(),"outbox":[],"mutations":[],"views":{},"uploads":[],"chats":{}
        })).unwrap();
        hosts.push(registry.insert(record).unwrap());
    }
    let unloaded_id = uuid::Uuid::new_v4().to_string();
    let unloaded: HostRecord = serde_json::from_value(serde_json::json!({
        "id":unloaded_id,"name":"Not connected yet","endpoint":"https://192.168.1.31:44330",
        "server_name":"paired-host","certificate":[],"fingerprint":[],"device_id":"device",
        "credential_ref":"never-read","ssh":null,"last_seen":null,"epoch":null,"closing_epoch":null,
        "catalog":[],"outbox":[],"mutations":[],"views":{},"uploads":[],"chats":{}
    }))
    .unwrap();
    let unloaded = registry.insert(unloaded).unwrap();
    let (app, local_chat) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(cx);
        ensure_global(cx).unwrap();
        cx.global::<ClientGlobal>()
            .registry
            .set(registry.clone())
            .unwrap_or_else(|_| panic!("new registry"));
        let app = AppState::install_test(Workspace::new(), cx);
        let root = std::path::PathBuf::from("/local/canopy");
        app.projects.update(cx, |projects, cx| {
            projects.ready = true;
            projects.open_worktree(root.clone(), root.clone(), cx);
            projects
                .open_chat(
                    projects.catalog.active,
                    "Local chat",
                    "claude",
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        title: Some("Local chat".into()),
                        cwd: Some(root.clone()),
                        ..Default::default()
                    },
                    cx,
                )
                .unwrap();
        });
        app.git.update(cx, |git, _| {
            git.entries.insert(
                root.clone(),
                Ok(Some(Arc::new(RepositoryInfo {
                    origin: RepositoryIdentity::from_origin(
                        "git@github.com:itsoltech/canopy-desktop.git",
                    ),
                    root: root.clone(),
                    common: root.join(".git"),
                    branches: vec!["main".into()],
                    worktrees: vec![WorktreeInfo {
                        name: None,
                        path: root,
                        head: WorktreeHead::Branch("main".into()),
                        label: "main".into(),
                        locked: false,
                        available: true,
                        missing: false,
                        registered_branch: None,
                    }],
                }))),
            );
        });
        let chat = app.workspace.read(cx).selected_pane().unwrap().id;
        (app, chat)
    });
    let selected = Rc::new(RefCell::new(None));
    let mut sidebar: Option<Entity<Sidebar>> = None;
    let mut watch: Option<Subscription> = None;
    let handle = cx.open_window(size(px(400.), px(1800.)), |window, cx| {
        let view = cx.new(Sidebar::new);
        let events = selected.clone();
        watch = Some(cx.subscribe(&view, move |_, _, event: &Navigation, _| {
            *events.borrow_mut() = Some(event.clone());
        }));
        sidebar = Some(view.clone());
        Root::new(view, window, cx)
    });
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        cx.global::<ClientGlobal>()
            .connections
            .read(cx)
            .hosts()
            .len()
            == 4
    })
    .await;
    let original = cx.update(|cx| app.projects.read(cx).catalog.active);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("project-origin:https://github.com/itsoltech/canopy-desktop")
                .visible()
        );
        assert!(
            window
                .find("project-origin:https://gitlab.com/itsoltech/canopy-desktop")
                .visible()
        );
        assert!(
            window
                .find(format!("remote-connect-{unloaded_id}"))
                .visible()
        );
        assert_eq!(
            unloaded.phase(),
            Phase::Offline,
            "showing an unloaded connection must not use its credentials"
        );
        // A project only another computer shares starts collapsed, like a
        // local one away from the selection.
        window.click(
            "project-origin:https://gitlab.com/itsoltech/canopy-desktop",
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    for host in &hosts {
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            let row = format!("worktree-remote-{}-main", host.id().as_str());
            let label = window.find(row).label().unwrap().to_owned();
            assert!(
                label.contains("main") && label.contains(&host.name()),
                "{label}"
            );
            window.click(
                format!("chat-remote-{}-same-chat-id", host.id().as_str()),
                cx,
            );
        })
        .unwrap();
        cx.run_until_parked();
        let event = selected.borrow();
        let Some(Navigation::Remote {
            host: target,
            resource,
        }) = event.as_ref()
        else {
            panic!("remote navigation")
        };
        assert_eq!(target.id(), host.id());
        let Some(p::resource_ref::Resource::Chat(chat)) = &resource.resource else {
            panic!("chat target")
        };
        assert_eq!(chat.host_id, host.id().as_str());
        cx.update(|cx| assert_eq!(app.projects.read(cx).catalog.active, original));
    }
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(format!("chat-{local_chat:?}"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(matches!(*selected.borrow(), Some(Navigation::Local)));
    drop(watch);
    drop(sidebar);
}
