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
