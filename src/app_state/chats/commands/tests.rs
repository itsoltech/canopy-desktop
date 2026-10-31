//! A fork is published in one transaction with its edited first message: a failure
//! at that point leaves no fork, no message and no partial receipt behind. A private
//! fork's message keeps its own copies of the files it attaches.
use super::{ForkChat, ForkReplacement, RemoteKey, fork_files};
use crate::app_state::AppState;
use crate::app_state::chats::ChatRuntime;
use canopy_desktop::{
    chat::{
        config::ChatSettings,
        session::Draft,
        transcript::{self as t, Attachment},
    },
    host::{CHAT, ChatRecord, CommandKey, CommandReceipt, CommandState, QueueDraft},
    settings::{ChatHistoryBatch, SettingsClient},
    state::workspace::{PaneKind, PaneMetadata, Workspace, WorkspaceId},
};
use gpui_kit::{App, Entity, Task, TestAppContext};
use std::path::{Path, PathBuf};

async fn settle(cx: &mut TestAppContext, done: impl Fn(&mut App) -> bool) {
    for _ in 0..500 {
        cx.run_until_parked();
        if cx.update(|cx| done(cx)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the app didn't settle");
}

async fn finish<T: 'static>(cx: &mut TestAppContext, task: Task<T>) -> T {
    let mut task = Box::pin(task);
    for _ in 0..500 {
        cx.run_until_parked();
        if let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task))
        {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the task didn't finish");
}

fn item(id: u64, body: t::Body) -> String {
    serde_json::to_string(&t::Item {
        id: t::ItemId(id),
        turn: t::TurnId(1),
        parent: None,
        body,
        trimmed: false,
    })
    .unwrap()
}

fn fork_of(
    app: &Entity<crate::app_state::chats::Chats>,
    source: &str,
    workspace: Option<WorkspaceId>,
    draft: QueueDraft,
    source_folder: Option<PathBuf>,
    cx: &mut TestAppContext,
) -> (String, CommandKey, Task<Result<CommandReceipt, String>>) {
    let chat = ChatSettings::new_id();
    let fingerprint = uuid::Uuid::new_v4().as_bytes().to_vec();
    let send = CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap();
    let fork = ForkChat {
        source: source.into(),
        history_epoch: 0,
        item: 2,
        chat: chat.clone(),
        workspace,
        replacement_fingerprint: Some(ForkReplacement::fingerprint(&fingerprint)),
    };
    let replacement = ForkReplacement {
        key: send.clone(),
        message: uuid::Uuid::new_v4().to_string(),
        draft,
        source_folder,
    };
    let key = RemoteKey {
        key: CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap(),
        fingerprint: Some(fingerprint),
    };
    let task = cx.update(|cx| {
        app.update(cx, |chats, cx| {
            chats.fork_chat(key, fork, Some(replacement), cx)
        })
    });
    (chat, send, task)
}

/// A host with an open project and a finished private chat (`source`, in folder
/// `source_folder`) that can be forked at its turn end.
struct Host {
    _folder: tempfile::TempDir,
    client: SettingsClient,
    app: AppState,
    workspace: WorkspaceId,
    checkout: PathBuf,
    source: String,
    source_folder: PathBuf,
    runtime: Entity<ChatRuntime>,
}

async fn host(cx: &mut TestAppContext) -> Host {
    cx.background_executor.allow_parking();
    let folder = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&folder.path().join("settings.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let checkout = folder.path().join("project");
    std::fs::create_dir(&checkout).unwrap();
    let checkout = std::fs::canonicalize(checkout).unwrap();
    let source_folder = folder.path().join("private");
    std::fs::create_dir(&source_folder).unwrap();
    let source_folder = std::fs::canonicalize(source_folder).unwrap();
    let source = ChatSettings::new_id();
    let record = ChatRecord::new(
        source.clone(),
        None,
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            cwd: Some(source_folder.clone()),
            chat: Some(Box::new(ChatSettings {
                id: Some(source.clone()),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let record = client.save_runtime_chat(record).await.unwrap();
    // A finished turn of an earlier session, with the point its agent can branch at.
    let meta = serde_json::json!({"agent": {"tool": "claude", "session": "session-1"}});
    client
        .save_chat_history(vec![ChatHistoryBatch {
            chat: source.clone(),
            epochs: vec![(0, meta.to_string()), (1, "{}".into())],
            items: vec![
                (
                    0,
                    1,
                    item(
                        1,
                        t::Body::Text(t::TextBlock {
                            text: "Done.".into(),
                            streaming: false,
                        }),
                    ),
                ),
                (
                    0,
                    2,
                    item(
                        2,
                        t::Body::TurnEnd(t::TurnEnd {
                            outcome: t::Outcome::Completed,
                            usage: None,
                            cost_usd: None,
                            duration_ms: None,
                            resume_at: Some("message-1".into()),
                            quiet: false,
                        }),
                    ),
                ),
            ],
        }])
        .await
        .unwrap();
    let (app, workspace, runtime) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings.update(cx, |settings, _| {
            settings.install_test_client(client.clone(), host);
            settings.record_saved(record);
        });
        app.projects.update(cx, |projects, _| {
            projects.install_test_client(client.clone())
        });
        app.chat_history
            .update(cx, |history, cx| history.initialize(client.clone(), cx));
        let workspace = app
            .projects
            .update(cx, |projects, cx| {
                projects.open_without_selecting(checkout.clone(), cx)
            })
            .unwrap();
        let runtime = app
            .chats
            .update(cx, |chats, cx| chats.ensure_runtime_for_chat(&source, cx))
            .unwrap();
        (app, workspace, runtime)
    });
    settle(cx, |cx| {
        let runtime = runtime.read(cx);
        !runtime.history_uncertain() && runtime.fork_request(0, 2).is_some()
    })
    .await;
    Host {
        _folder: folder,
        client,
        app,
        workspace,
        checkout,
        source,
        source_folder,
        runtime,
    }
}

/// Fork `chat` failed and nothing of it remains: no record, runtime or message.
async fn nothing_published(host: &Host, chat: &str, send: CommandKey, cx: &mut TestAppContext) {
    let chats = host.client.load_runtime_chats().await.unwrap();
    assert!(!chats.iter().any(|r| r.id() == chat));
    assert!(
        host.client
            .get_runtime_command(send, CHAT)
            .await
            .unwrap()
            .is_none()
    );
    cx.update(|cx| {
        assert!(host.app.settings.read(cx).chat_record(chat).is_none());
        assert!(host.app.chats.read(cx).runtime_for_chat(chat).is_none());
    });
}

/// The private chat folders the fork path has made and not removed.
fn private_folders() -> Vec<PathBuf> {
    let root = canopy_desktop::chat::scratch::root(&super::chats_data().unwrap());
    std::fs::read_dir(root)
        .map(|entries| entries.map(|entry| entry.unwrap().path()).collect())
        .unwrap_or_default()
}

fn reason_of(receipt: &CommandReceipt) -> String {
    let result: serde_json::Value = serde_json::from_slice(receipt.result().unwrap()).unwrap();
    result["reason"].as_str().unwrap().to_owned()
}

fn with_files(paths: &[&Path]) -> QueueDraft {
    QueueDraft::from(Draft {
        text: "edited".into(),
        attachments: paths
            .iter()
            .map(|path| Attachment::Path {
                path: path.to_path_buf(),
                label: None,
            })
            .collect(),
        context: None,
        goal: false,
    })
}

#[gpui_kit::test]
async fn a_fork_and_its_edited_message_are_published_together_or_not_at_all(
    cx: &mut TestAppContext,
) {
    let host = host(cx).await;
    // The edited message can't be accepted (its image is no upload of this host):
    // nothing of the fork remains, and the command failed instead of succeeding
    // with a note that its message is missing.
    let unknown = QueueDraft::from(Draft::text("edited"))
        .with_artifacts(vec![uuid::Uuid::new_v4().to_string()])
        .unwrap();
    let (failed, send, task) = fork_of(
        &host.app.chats,
        &host.source,
        Some(host.workspace),
        unknown,
        None,
        cx,
    );
    let receipt = finish(cx, task).await.unwrap();
    assert_eq!(receipt.state(), CommandState::Failed);
    nothing_published(&host, &failed, send, cx).await;

    // A valid one: the fork is listed with its edited message already queued.
    let (forked, send, task) = fork_of(
        &host.app.chats,
        &host.source,
        Some(host.workspace),
        QueueDraft::from(Draft::text("edited")),
        None,
        cx,
    );
    let receipt = finish(cx, task).await.unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    let client = &host.client;
    let saved = client
        .load_runtime_chats()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id() == forked)
        .expect("the fork is in the catalog");
    assert_eq!(saved.authority_root(), host.checkout.to_str().unwrap());
    let queue = client.load_runtime_queue(forked.clone()).await.unwrap();
    assert_eq!(queue.entries().len(), 1);
    assert_eq!(queue.entries()[0].draft().text(), "edited");
    assert!(
        client
            .get_runtime_command(send, CHAT)
            .await
            .unwrap()
            .is_some()
    );
    cx.update(|cx| {
        let settings = host.app.settings.read(cx);
        let record = settings.chat_record(&forked).unwrap();
        assert!(saved.thread_id().is_some());
        assert_eq!(record.thread(), saved.thread());
        let summaries = host.app.chats.read(cx).chat_summaries(cx);
        assert_eq!(
            summaries
                .iter()
                .find(|s| s.chat == forked)
                .unwrap()
                .thread_id,
            saved.thread_id()
        );
    });
    client.shutdown().await.unwrap();
}

/// A private fork's edited message names files of its source's folder: they are
/// copied into the fork's own folder before it is published, so the accepted message
/// still has them once the source is gone. A link out of the folder, a special file
/// or too many files fail the fork with nothing published or left behind.
#[gpui_kit::test]
async fn a_private_fork_keeps_copies_of_exactly_the_files_its_message_attaches(
    cx: &mut TestAppContext,
) {
    let host = host(cx).await;
    let src = host.source_folder.clone();
    std::fs::create_dir_all(src.join("notes/deep")).unwrap();
    std::fs::write(src.join("notes/a.txt"), "alpha").unwrap();
    std::fs::write(src.join("notes/deep/b.txt"), "beta").unwrap();
    std::fs::write(src.join("top.txt"), "top").unwrap();
    std::fs::write(src.join("unattached.txt"), "stays behind").unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let before = private_folders();

    let mut refused: Vec<PathBuf> = vec![src.join("../elsewhere.txt"), src.join("missing.txt")];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), src.join("escape")).unwrap();
        std::fs::create_dir(src.join("linked")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            src.join("linked/secret.txt"),
        )
        .unwrap();
        let fifo = std::ffi::CString::new(src.join("pipe").to_str().unwrap()).unwrap();
        // SAFETY: a NUL-terminated path of the test's own folder.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        refused.extend([
            src.join("escape/secret.txt"),
            src.join("escape"),
            src.join("linked"),
            src.join("pipe"),
        ]);
    }
    // More files than one fork may copy.
    std::fs::create_dir(src.join("many")).unwrap();
    for n in 0..fork_files::BOUND.entries {
        std::fs::write(src.join("many").join(n.to_string()), "").unwrap();
    }
    refused.push(src.join("many"));
    for path in refused {
        let draft = with_files(&[&src.join("top.txt"), &path]);
        let (failed, send, task) = fork_of(
            &host.app.chats,
            &host.source,
            None,
            draft,
            Some(src.clone()),
            cx,
        );
        let receipt = finish(cx, task).await.unwrap();
        assert_eq!(receipt.state(), CommandState::Failed, "{}", path.display());
        nothing_published(&host, &failed, send, cx).await;
        assert_eq!(private_folders(), before, "{}", path.display());
    }
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "secret"
    );

    // Attached: a folder, a file inside it (overlapping) and a file at the top.
    let draft = with_files(&[
        &src.join("notes"),
        &src.join("notes/deep/b.txt"),
        &src.join("top.txt"),
    ]);
    let (forked, send, task) = fork_of(
        &host.app.chats,
        &host.source,
        None,
        draft,
        Some(src.clone()),
        cx,
    );
    let receipt = finish(cx, task).await.unwrap();
    assert_eq!(receipt.state(), CommandState::Succeeded);
    assert!(
        host.client
            .get_runtime_command(send, CHAT)
            .await
            .unwrap()
            .is_some()
    );
    let saved = host
        .client
        .load_runtime_chats()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id() == forked)
        .expect("the fork is in the catalog");
    let cwd = saved.metadata().cwd.clone().unwrap();
    assert_ne!(cwd, src);
    let queue = host
        .client
        .load_runtime_queue(forked.clone())
        .await
        .unwrap();
    let paths: Vec<_> = queue.entries()[0]
        .draft()
        .attachments()
        .iter()
        .map(|attachment| match attachment {
            Attachment::Path { path, .. } => path.clone(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        paths,
        [
            cwd.join("notes"),
            cwd.join("notes/deep/b.txt"),
            cwd.join("top.txt")
        ]
    );
    let mut copied: Vec<_> = std::fs::read_dir(&cwd)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    copied.sort();
    assert_eq!(copied, ["notes", "top.txt"]);

    // The source's folder goes: the fork's message keeps what it was accepted with.
    std::fs::remove_dir_all(&src).unwrap();
    assert_eq!(
        std::fs::read_to_string(cwd.join("notes/a.txt")).unwrap(),
        "alpha"
    );
    assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "beta");
    assert_eq!(std::fs::read_to_string(&paths[2]).unwrap(), "top");
    canopy_desktop::chat::scratch::remove(&super::chats_data().unwrap(), &cwd).unwrap();
    host.client.shutdown().await.unwrap();
}

/// Stop names a card by history part and item: one from an earlier part is refused
/// before the card is looked up, since the same item number may now be another
/// running command of the current part.
#[gpui_kit::test]
async fn a_stop_for_a_card_of_another_history_part_is_refused(cx: &mut TestAppContext) {
    let host = host(cx).await;
    let stop = |history_epoch: u64, cx: &mut TestAppContext| {
        host.runtime.update(cx, |runtime, cx| {
            let key = RemoteKey::from(CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap());
            let (run, epoch) = (runtime.generation(), runtime.epoch());
            runtime.remote_stop_shell(key, run, epoch.wrapping_add(history_epoch), 2, cx)
        })
    };
    let task = stop(1, cx);
    let stale = finish(cx, task).await.unwrap();
    assert_eq!(stale.state(), CommandState::Failed);
    assert!(
        reason_of(&stale).contains("another history part"),
        "{}",
        reason_of(&stale)
    );
    let task = stop(0, cx);
    let current = finish(cx, task).await.unwrap();
    assert_eq!(current.state(), CommandState::Failed);
    assert!(
        reason_of(&current).contains("no longer running"),
        "{}",
        reason_of(&current)
    );
    host.client.shutdown().await.unwrap();
}
