use super::*;
use canopy_desktop::integrations::attachments::{PREVIEW_LIMIT, PreviewFile};
use prost::Message;
use sha2::{Digest, Sha256};
impl TasksView {
    /// The native picker supplies the path; the prepared upload remains bound
    /// to the task that was open when the file was chosen.
    pub(super) fn pick_attachment(&mut self, cx: &mut Context<Self>) {
        let Ok(task) = self.task_ref() else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Attach file".into()),
        });
        cx.spawn(async move |this, cx| {
            let picked = selection.await;
            let _ = this.update(cx, |s, cx| match picked {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next()
                        && s.task_ref().is_ok_and(|t| t == task)
                    {
                        s.prepare_attachment(path, cx);
                    }
                }
                Ok(Err(e)) => s.fail(e.to_string(), cx),
                _ => {}
            });
        })
        .detach();
    }
    pub(super) fn prepare_attachment(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let task = match self.task_ref() {
            Ok(t) => t,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let host = self.host.clone();
        self.run(
            async move {
                let (id, name) = tokio::task::spawn_blocking({
                    let task = task.clone();
                    move || {
                        use std::io::Read;
                        let mut bytes = vec![];
                        std::fs::File::open(&path)
                            .map_err(|e| e.to_string())?
                            .take(PREVIEW_LIMIT as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        if bytes.len() > PREVIEW_LIMIT {
                            return Err("Choose a file up to 25 MiB.".into());
                        }
                        let name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or("Choose a file with a valid name.")?
                            .to_owned();
                        let media = match path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase()
                            .as_str()
                        {
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            "gif" => "image/gif",
                            "webp" => "image/webp",
                            "pdf" => "application/pdf",
                            "txt" | "md" => "text/plain",
                            _ => "application/octet-stream",
                        };
                        let id = host.prepare_task_attachment(
                            task.clone(),
                            media,
                            &bytes,
                        )?;
                        host.save_view(format!("task-upload:{id}"),serde_json::json!({"task":task.encode_to_vec(),"name":name,"attempted":false}).to_string())?;
                        Ok::<_, String>((id, name))
                    }
                })
                .await
                .map_err(|e| e.to_string())??;
                Ok(ResultView::UploadPrepared(id, task, name))
            },
            cx,
        );
    }
    pub(super) fn upload_attachment(&mut self, cx: &mut Context<Self>) {
        let Some((id, task, name)) = self.pending_upload.clone() else {
            return;
        };
        let host = self.host.clone();
        self.upload_attempted = true;
        self.run_as(
            Loading::Save,
            async move {
                let artifact = host.resume_upload(&id).await?;
                host.save_view(format!("task-upload:{id}"),serde_json::json!({"task":task.encode_to_vec(),"name":name,"attempted":true}).to_string())?;
                let project = task.project.clone().ok_or("Task project is missing.")?;
                let receipt = host
                    .mutate_task(TaskMutation::Apply(p::ApplyMutationRequest {
                                command: None,
                                project: Some(project),
                                task_id: Some(task.task_id),
                                action: Some(Mutation::Attachment(p::TaskAttachmentWrite {
                                    action: Some(p::task_attachment_write::Action::Attach(
                                        p::ArtifactRef { host_id: artifact.host_id, artifact_id: artifact.artifact_id, media_type: artifact.media_type, size: artifact.size, sha256: artifact.sha256 },
                                    )),
                                    name,
                                })),
                            }))
                    .await?;
                let cleanup=host.remove_local_attachment(&id);
                let mut receipt=receipt;
                if let Err(error)=cleanup && let Some(p::command_receipt::Result::Task(result)) = receipt.result.as_mut() { result.notice=Some(format!("Attachment added. Local cleanup failed: {error}. Do not repeat attachment creation.")); }
                let _=host.save_view(format!("task-upload:{id}"),"".into());
                Ok(ResultView::UploadAttached(receipt))
            },
            cx,
        );
    }
    pub(super) fn preview_attachment(
        &mut self,
        attachment: p::TaskAttachment,
        cx: &mut Context<Self>,
    ) {
        let task = match self.task_ref() {
            Ok(t) => t,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let host = self.host.clone();
        self.run(
            async move {
                let mut stream = Client::new(host.channel()?)
                    .read_attachment(p::ReadAttachmentRequest {
                        task: Some(task),
                        attachment_id: attachment.id,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                let mut bytes = Vec::new();
                let mut identity: Option<p::ArtifactRef> = None;
                let mut complete = false;
                while let Some(chunk) = stream.message().await.map_err(rpc_error)? {
                    let artifact = chunk.artifact.ok_or("Attachment chunk has no identity.")?;
                    if artifact.host_id != host.id().as_str()
                        || artifact.size > PREVIEW_LIMIT as u64
                        || artifact.sha256.len() != 32
                        || identity.as_ref().is_some_and(|a| a != &artifact)
                        || complete
                        || chunk.offset != bytes.len() as u64
                        || bytes.len().saturating_add(chunk.data.len()) > PREVIEW_LIMIT
                        || chunk.offset.saturating_add(chunk.data.len() as u64) > artifact.size
                    {
                        return Err("Attachment stream identity, bounds or offset changed.".into());
                    }
                    if chunk.data.is_empty() && !chunk.complete {
                        return Err("Attachment stream made no progress.".into());
                    }
                    identity = Some(artifact);
                    bytes.extend_from_slice(&chunk.data);
                    complete = chunk.complete;
                }
                let identity = identity.ok_or("Attachment stream was empty.")?;
                if !complete
                    || bytes.len() as u64 != identity.size
                    || Sha256::digest(&bytes).as_slice() != identity.sha256
                {
                    return Err("Attachment download is incomplete or its checksum differs.".into());
                }
                let file = tokio::task::spawn_blocking(move || {
                    PreviewFile::create(&attachment.name, &bytes)
                })
                .await
                .map_err(|e| e.to_string())??;
                Ok(ResultView::Preview(Arc::new(file), identity.media_type))
            },
            cx,
        );
    }
}
impl TasksView {
    pub(super) fn restore_upload(&mut self) {
        let record = self.host.snapshot().record;
        for id in self.host.upload_journal() {
            let Some(value) = record
                .view(&format!("task-upload:{id}"))
                .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
            else {
                continue;
            };
            let Some(bytes) = value["task"].as_array().and_then(|v| {
                v.iter()
                    .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                    .collect::<Option<Vec<_>>>()
            }) else {
                continue;
            };
            let Ok(task) = p::TaskRef::decode(bytes.as_slice()) else {
                continue;
            };
            if task
                .project
                .as_ref()
                .and_then(|p| p.resource.as_ref())
                .is_none_or(|p| match p {
                    p::task_project::Resource::Project(p) => p.host_id != self.host.id().as_str(),
                    p::task_project::Resource::Environment(e) => {
                        e.host_id != self.host.id().as_str()
                    }
                })
            {
                continue;
            }
            let Some(name) = value["name"].as_str() else {
                continue;
            };
            self.pending_upload = Some((id, task, name.into()));
            self.upload_attempted = value["attempted"].as_bool().unwrap_or(true);
            break;
        }
    }
    pub(super) fn confirm_upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.upload_attempted {
            self.upload_attachment(cx);
            return;
        }
        let owner = cx.entity().downgrade();
        let name = self
            .pending_upload
            .as_ref()
            .map(|u| u.2.clone())
            .unwrap_or_default();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .title(format!("Upload “{name}” again?"))
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text("Upload again")
                        .show_cancel(true),
                )
                .child(
                    "An earlier upload may already have attached this file. Refresh the task \
                     first; uploading again can create a duplicate attachment.",
                )
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |s, cx| s.upload_attachment(cx));
                    true
                })
        });
    }
    pub(super) fn discard_upload(&mut self, cx: &mut Context<Self>) {
        let Some((id, _, _)) = self.pending_upload.clone() else {
            return;
        };
        let host = self.host.clone();
        self.run(
            async move {
                host.remove_local_attachment(&id)?;
                let _ = host.save_view(format!("task-upload:{id}"), "".into());
                Ok(ResultView::UploadDiscarded)
            },
            cx,
        );
    }
}
