use super::*;
use canopy_desktop::integrations::attachments::{PREVIEW_LIMIT, PreviewFile};
use prost::Message;
use sha2::{Digest, Sha256};
impl TasksView {
    pub(super) fn prepare_attachment(&mut self, cx: &mut Context<Self>) {
        let task = match self.task_ref() {
            Ok(t) => t,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let path = std::path::PathBuf::from(value(&self.attachment_path, cx));
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
                            return Err("Attachment exceeds 25 MiB.".into());
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
                        let id = host.prepare_attachment(
                            p::ResourceRef {
                                resource: Some(p::resource_ref::Resource::Task(task.clone())),
                            },
                            "task_attachment",
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
        self.run(
            async move {
                let artifact = host.resume_upload(&id).await?;
                host.save_view(format!("task-upload:{id}"),serde_json::json!({"task":task.encode_to_vec(),"name":name,"attempted":true}).to_string())?;
                let project = p::TaskProject {
                    host_id: task.host_id.clone(),
                    integration_id: task.integration_id.clone(),
                    project_id: task.project_id.clone(),
                    workspace: task.workspace.clone(),
                };
                let receipt = host
                    .mutate(None, move |channel, command| async move {
                        Client::new(channel)
                            .apply_mutation(p::TaskMutationRequest {
                                command: Some(command),
                                project: Some(project),
                                task: Some(task),
                                action: Some(Mutation::Attachment(p::TaskAttachmentWrite {
                                    action: Some(p::task_attachment_write::Action::Attach(
                                        artifact,
                                    )),
                                    name,
                                })),
                            })
                            .await
                    })
                    .await?;
                let cleanup=host.remove_local_attachment(&id);
                let mut receipt=receipt;
                if let Err(error)=cleanup{receipt.operation_output=Some(serde_json::json!({"notice":format!("Attachment added. Local cleanup failed: {error}. Do not repeat attachment creation.")}).to_string());}
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
                    .read_attachment(p::TaskAttachmentRequest {
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
            if task.host_id != self.host.id().as_str() {
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
        window.open_dialog(cx,move|dialog,_,_|{let owner=owner.clone();dialog.title("Retry attachment creation?")
            .button_props(gpui_kit::component::dialog::DialogButtonProps::default().ok_text("Create another attempt").show_cancel(true))
            .child("An earlier attempt may already have attached this file. Query its command result and refresh the original task first. If the outcome is still unknown, this explicit new attempt can create a duplicate. Upload bytes always remain bound to the original task.")
            .on_ok(move|_,_,cx|{let _=owner.update(cx,|s,cx|s.upload_attachment(cx));true})});
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
