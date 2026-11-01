use super::*;
use canopy_desktop::chat::{claude::wire, process::Exit, transcript::Attachment};
use canopy_desktop::state::workspace::{PaneId, PaneMetadata, Workspace};
use core::prelude::v1::test;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};

fn feed(session: &mut ChatSession, value: Value) {
    session.on_message(
        wire::parse_line(value.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    );
}

fn ready() -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        json!({"type":"control_response","response":{
        "subtype":"success","request_id":"canopy-1","response":{}}}),
    );
    feed(
        &mut session,
        json!({"type":"system","subtype":"init","session_id":"session"}),
    );
    session
}

fn finish_turn(session: &mut ChatSession, reply: &str) {
    feed(
        session,
        json!({"type":"assistant","uuid":reply,"message":{
        "id":reply,"role":"assistant","content":[{"type":"text","text":"done"}]}}),
    );
    feed(
        session,
        json!({"type":"result","subtype":"success","is_error":false,"result":"done"}),
    );
    feed(
        session,
        json!({"type":"system","subtype":"session_state_changed","state":"idle"}),
    );
}

fn first_user(session: &ChatSession) -> ItemId {
    session
        .transcript()
        .items()
        .iter()
        .find(|item| matches!(item.body, Body::User(_)))
        .unwrap()
        .id
}

fn runtime(cx: &mut App) -> Entity<ChatRuntime> {
    AppState::install_test(Workspace::new(), cx);
    cx.new(|cx| {
        ChatRuntime::new(
            Pane {
                id: PaneId::new(),
                tool: "claude".into(),
                metadata: PaneMetadata::default(),
            },
            cx,
        )
    })
}

#[gpui_kit::test]
fn local_edit_selection_is_silent_and_does_not_add_host_command_preconditions(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let runtime = runtime(cx);
        runtime.update(cx, |runtime, cx| {
            assert!(runtime.preview_message_edit(ItemId(999), cx).is_none());
            let mut continued = ready();
            continued.set_base(canopy_desktop::chat::session::Base::Continued);
            continued.submit("continues unknown earlier history");
            let item = first_user(&continued);
            runtime.session = Some(BackendSession::Native(Box::new(continued)));
            assert!(runtime.preview_message_edit(item, cx).is_none());
            assert!(runtime.notice.is_none());

            let mut session = ready();
            let sent = session.submit_draft(Draft { text: "original".into(), context: Some("original context".into()), ..Default::default() });
            let item = first_user(&session);
            // These are host command guards, not additional local pencil guards.
            let uuid = sent.iter().find(|line| line["type"] == "user").unwrap()["uuid"].clone();
            feed(&mut session, json!({"type":"command_lifecycle","command_uuid":uuid,"state":"queued"}));
            session.steer_text("still queued");
            feed(&mut session, json!({"type":"control_request","request_id":"permission","request":{
                "subtype":"can_use_tool","tool_name":"Read","input":{"file_path":"/project/file"},"tool_use_id":"read"}}));
            runtime.session = Some(BackendSession::Native(Box::new(session)));
            runtime.recovery = Some("choose an account".into());
            runtime.starting = true;
            runtime.switching = true;
            runtime.send_on_start = Some(Draft::text("waiting"));
            assert!(!runtime.pending_decisions().is_empty());
            assert!(!runtime.queue_lines().is_empty());
            assert!(runtime.edit_target(runtime.generation, runtime.epoch, item.0, cx).is_err());
            let selection = runtime.preview_message_edit(item, cx).expect("the original local selection still opens");
            assert!(selection.checking.is_none());
            assert_eq!(selection.original.unwrap().text, "original");
            let saved_image = Draft { attachments: vec![image(None)], ..Draft::text("edited image") };
            assert!(matches!(runtime.commit_message_edit(&selection.target, saved_image, false, 1, cx), EditSubmission::Preparing(_)), "the edit must keep its composer while saved images load");
            let EditSubmission::Restart(branch) = runtime.commit_message_edit(
                &selection.target, Draft::text("edited"), false, 1, cx) else { panic!("local edit must be ready before restart"); };
            assert!(branch.fork.is_none());
            assert_eq!(branch.draft.text, "edited");
            assert_eq!(branch.draft.context.as_deref(), Some("original context"));

            runtime.generation += 1;
            let EditSubmission::Refused { message, recheck } = runtime.commit_message_edit(
                &selection.target, Draft::text("late"), false, 1, cx) else { panic!("an old selection cannot edit a new run"); };
            assert_eq!(message, "The chat restarted since; open the message again.");
            assert_eq!(message, crate::app_state::remote::edit_reason("stale_run"));
            assert!(!recheck);
        });
        drop(runtime);
    });
}

#[gpui_kit::test]
fn local_edit_preserves_restore_progress_errors_and_checks_resume_only_before_fork(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let runtime = runtime(cx);
        runtime.update(cx, |runtime, cx| {
            let mut session = ready();
            session.submit("first");
            finish_turn(&mut session, "reply-1");
            session.submit("second");
            finish_turn(&mut session, "reply-2");
            let item = session.transcript().items().iter().rev().find(|item| matches!(item.body, Body::User(_))).unwrap().id;
            runtime.session = Some(BackendSession::Native(Box::new(session)));
            let selection = runtime.preview_message_edit(item, cx).expect("missing resume metadata does not block opening");
            let preview = runtime.outbox.last().unwrap()["request_id"].clone();
            feed(runtime.native_mut().unwrap(), json!({"type":"control_response","response":{
                "subtype":"error","request_id":preview,"error":"conditions_changed: checkpoints unavailable"}}));
            assert!(matches!(runtime.message_edit_progress(&selection.target, false), EditProgress::Preview(preview) if !preview.possible));
            // Dropping a preview observation must not strand the next request.
            assert!(runtime.preview_message_edit(item, cx).unwrap().checking.is_none());
            let preview = runtime.outbox.last().unwrap()["request_id"].clone();
            feed(runtime.native_mut().unwrap(), json!({"type":"control_response","response":{
                "subtype":"success","request_id":preview,"response":{"canRewind":true,"filesChanged":["file.rs"]}}}));
            assert!(matches!(runtime.message_edit_progress(&selection.target, false), EditProgress::Preview(preview) if preview.possible && preview.files == ["file.rs"]));
            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), true, 1, cx), EditSubmission::Restoring));
            let restore = runtime.outbox.last().unwrap()["request_id"].clone();
            assert!(matches!(runtime.message_edit_progress(&selection.target, true), EditProgress::Unchanged));
            feed(runtime.native_mut().unwrap(), json!({"type":"control_response","response":{
                "subtype":"error","request_id":restore,"error":"checkpoint missing"}}));
            assert!(matches!(runtime.message_edit_progress(&selection.target, true), EditProgress::RestoreFailed(error) if error == "Couldn't restore the files: checkpoint missing"));

            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), true, 1, cx), EditSubmission::Restoring));
            let restore = runtime.outbox.last().unwrap()["request_id"].clone();
            feed(runtime.native_mut().unwrap(), json!({"type":"control_response","response":{
                "subtype":"success","request_id":restore,"response":{"canRewind":true}}}));
            assert!(matches!(runtime.message_edit_progress(&selection.target, true), EditProgress::Restored));
            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), false, 1, cx), EditSubmission::Refused { message, recheck: false } if message == "This conversation can't be continued from there."));
            runtime.pane.metadata.resume_id = Some("session".into());
            let EditSubmission::Restart(branch) = runtime.commit_message_edit(&selection.target, Draft::text("edited"), false, 1, cx) else { panic!("the branch is ready, without waiting for a command receipt"); };
            assert_eq!(branch.fork.as_deref(), Some("reply-1"));

            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), true, 1, cx), EditSubmission::Restoring));
            runtime.native_mut().unwrap().on_event(Event::Exited(Exit { code: Some(0), signal: None, stderr: String::new() }));
            assert!(matches!(runtime.message_edit_progress(&selection.target, true), EditProgress::RestoreFailed(error) if error == "The chat ended before the files were restored."));
            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), true, 1, cx), EditSubmission::RestoreFailed(error) if error == "Files can't be restored now."));
            // It can still send the edit without restoring files.
            assert!(matches!(runtime.commit_message_edit(&selection.target, Draft::text("edited"), false, 1, cx), EditSubmission::Restart(_)));
        });
        drop(runtime);
    });
}

fn image(data: Option<&str>) -> Attachment {
    Attachment::Image {
        file: "saved.png".into(),
        media_type: "image/png".into(),
        width: 1,
        height: 1,
        data: data.map(Arc::from),
        label: Some("[saved.png]".into()),
    }
}

#[gpui_kit::test]
fn composer_operations_keep_synchronous_clearing_and_defer_only_saved_images(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let runtime = runtime(cx);
        runtime.update(cx, |runtime, cx| {
            let normal = runtime.prepare_composer_draft(Draft::text("send"), false);
            assert!(matches!(normal.clear, ComposerClear::WhenReady));
            assert!(
                matches!(
                    runtime.prepare_draft(normal.draft, cx),
                    DraftPreparation::Ready(_)
                ),
                "ordinary Send must not yield before clearing"
            );
            let in_memory = Draft {
                attachments: vec![image(Some("cGl4ZWxz"))],
                ..Draft::text("image")
            };
            assert!(matches!(
                runtime.prepare_draft(in_memory, cx),
                DraftPreparation::Ready(_)
            ));
            let saved = Draft {
                attachments: vec![image(None)],
                ..Draft::text("saved")
            };
            let normal = runtime.prepare_composer_draft(saved.clone(), false);
            assert!(matches!(normal.clear, ComposerClear::WhenReady));
            assert!(matches!(
                runtime.prepare_draft(normal.draft, cx),
                DraftPreparation::Loading(_)
            ));
            let steer = runtime.prepare_composer_draft(saved.clone(), true);
            assert!(matches!(steer.clear, ComposerClear::BeforePreparation));
            assert!(matches!(
                runtime.prepare_draft(steer.draft, cx),
                DraftPreparation::Loading(_)
            ));
            let goal = runtime.prepare_composer_draft(
                Draft {
                    goal: true,
                    ..Draft::text("goal")
                },
                false,
            );
            assert!(matches!(goal.clear, ComposerClear::BeforePreparation));
            assert!(matches!(
                runtime.prepare_draft(goal.draft, cx),
                DraftPreparation::Ready(_)
            ));
            assert_eq!(
                runtime
                    .submit_composer_shell(String::new(), cx)
                    .err()
                    .as_deref(),
                Some("Type a command to run.")
            );
            runtime.generation += 1;
            assert!(!runtime.composer_submission_current(&steer.submission));
            let (clear, taken) = runtime.take_queued("missing".into(), Draft::default(), cx);
            assert!(matches!(clear, ComposerClear::BeforePreparation));
            drop(taken);

            let reference = canopy_desktop::state::workspace::RemoteChat::new(
                uuid::Uuid::new_v4().to_string(),
                uuid::Uuid::new_v4().to_string(),
            )
            .unwrap();
            runtime.transport = ThreadTransport::Grpc(Box::new(RemoteSession::new(reference)));
            let hosted = runtime.prepare_composer_draft(saved, true);
            assert!(matches!(hosted.clear, ComposerClear::OnAcceptance));
            assert!(
                matches!(
                    runtime.prepare_draft(hosted.draft, cx),
                    DraftPreparation::Ready(_)
                ),
                "host artifact bytes must never be materialized on the device"
            );
            let (clear, taken) = runtime.take_queued("missing".into(), Draft::default(), cx);
            assert!(matches!(clear, ComposerClear::OnAcceptance));
            drop(taken);
        });
        drop(runtime);
    });
}

#[test]
fn saved_images_materialize_before_submission_and_missing_files_preserve_the_failure() {
    let folder = tempfile::tempdir().unwrap();
    let draft = Draft {
        attachments: vec![image(None)],
        ..Draft::text("saved image")
    };
    assert_eq!(
        materialize_draft_images(folder.path(), draft.clone()).unwrap_err(),
        "An attached image is no longer available."
    );
    std::fs::write(folder.path().join("saved.png"), b"pixels").unwrap();
    let loaded = materialize_draft_images(folder.path(), draft).unwrap();
    assert!(!canopy_desktop::chat::attachments::needs_data(
        &loaded.attachments
    ));
    assert!(
        matches!(&loaded.attachments[0], Attachment::Image { data: Some(data), .. } if data.as_ref() == "cGl4ZWxz")
    );
    assert_eq!(loaded.text, "saved image");
}
