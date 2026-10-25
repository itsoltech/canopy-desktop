use super::*;
use canopy_desktop::remote::client::rpc_error;
impl RemoteWindow {
    pub(super) fn open_remote_file(&mut self, file: p::FileRef, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.run(
            async move {
                let (bytes, hash) = host.read_file(file.clone()).await?;
                let media = super::preview::media_type(&file.relative_path, &bytes);
                if !media.starts_with("text/") {
                    let preview = canopy_desktop::integrations::attachments::PreviewFile::create(
                        &file.relative_path,
                        &bytes,
                    )?;
                    return Ok(Update::Preview(Arc::new(preview), media.into()));
                }
                let text = String::from_utf8(bytes).map_err(|_| "File is not UTF-8.")?;
                let saved = host
                    .snapshot()
                    .record
                    .view(&file_view_key(&file))
                    .and_then(|v| serde_json::from_str::<(Vec<u8>, String)>(v).ok());
                if let Some((base, draft)) = saved {
                    let dirty = base != hash || draft != text;
                    Ok(Update::File(file, base, draft, dirty))
                } else {
                    Ok(Update::File(file, hash, text, false))
                }
            },
            cx,
        );
    }

    pub(super) fn load_directory(&mut self, cx: &mut Context<Self>) {
        self.load_directory_page(None, cx);
    }
    fn load_directory_page(&mut self, token: Option<String>, cx: &mut Context<Self>) {
        let (Some(host), Some(workspace)) = (self.host.clone(), self.workspace.clone()) else {
            return;
        };
        if !self.ready() {
            return;
        }
        let relative_path = self.value("path", cx);
        self.run(
            async move {
                let append = token.is_some();
                let result = p::files_service_client::FilesServiceClient::new(host.channel()?)
                    .list_directory(p::ListDirectoryRequest {
                        directory: Some(p::FileRef {
                            workspace: Some(workspace),
                            relative_path,
                        }),
                        page_token: token,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                Ok(if append {
                    Update::DirectoryPage(result)
                } else {
                    Update::Directory(result)
                })
            },
            cx,
        );
    }
    pub(super) fn render_files(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.workspace.is_none() {
            return div()
                .child("Select a workspace in Projects.")
                .into_any_element();
        }
        let mut body = column()
            .gap_3()
            .child(self.field("path", "Workspace-relative path"))
            .child(
                button("list-files", "Open directory")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| this.load_directory(cx))),
            );
        body = body
            .child(self.field("query", "Search workspace files"))
            .child(
                button("search-files", "Search files")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        let workspace = this.workspace.clone();
                        let query = this.value("query", cx);
                        this.run(
                            async move {
                                Ok(Update::Directory(
                                    p::files_service_client::FilesServiceClient::new(
                                        host.channel()?,
                                    )
                                    .search_files(p::SearchFilesRequest {
                                        workspace,
                                        query,
                                        limit: 200,
                                    })
                                    .await
                                    .map_err(rpc_error)?
                                    .into_inner(),
                                ))
                            },
                            cx,
                        );
                    })),
            );
        for (directory, label) in [(false, "Create empty file…"), (true, "Create directory…")] {
            body = body.child(
                button(
                    SharedString::from(format!("create-file:{directory}")),
                    label,
                )
                .disabled(!self.ready())
                .on_click(cx.listener(move |this, _, _, cx| {
                    let host = this.host.clone().unwrap();
                    let file = p::FileRef {
                        workspace: this.workspace.clone(),
                        relative_path: this.value("path", cx),
                    };
                    this.apply(
                        Update::Confirm(
                            "Create workspace entry?".into(),
                            file.relative_path.clone(),
                            Box::pin(async move {
                                host.mutate(None, move |channel, command| async move {
                                    p::files_service_client::FilesServiceClient::new(channel)
                                        .create_file(p::CreateFileRequest {
                                            command: Some(command),
                                            file: Some(file),
                                            directory,
                                            content: None,
                                        })
                                        .await
                                })
                                .await?;
                                Ok(Update::Done(
                                    "Workspace entry created. Refresh the directory.".into(),
                                ))
                            }),
                        ),
                        cx,
                    );
                })),
            );
        }
        if let Some(directory) = self.directory.clone() {
            if directory.incomplete {
                body = body.child("Directory results are incomplete.");
            }
            if let Some(token) = directory.next_page_token.clone() {
                body = body.child(
                    button("directory-more", "Load more entries")
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_directory_page(Some(token.clone()), cx)
                        })),
                );
            }
            if let Some(warning) = directory.warning {
                body = body.child(warning);
            }
            for entry in directory.entries {
                let path = entry.relative_path;
                body = body.child(
                    button(
                        SharedString::from(format!("file:{path}")),
                        format!("{}{}", if entry.directory { "▸ " } else { "" }, entry.name),
                    )
                    .disabled(!self.ready())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if entry.directory {
                            this.fields["path"]
                                .update(cx, |s, cx| s.set_value(path.clone(), window, cx));
                            this.load_directory(cx);
                        } else {
                            let file = p::FileRef {
                                workspace: this.workspace.clone(),
                                relative_path: path.clone(),
                            };
                            this.open_remote_file(file, cx);
                        }
                    })),
                );
            }
        }
        if let Some(file) = self.file.clone() {
            body = body
                .child(file.relative_path.clone())
                .child(
                    button("preview-text-file", "Preview…").on_click(cx.listener(
                        |this, _, _, cx| {
                            let file = this.file.as_ref().unwrap();
                            let text = this.file_text.read(cx).value().to_string();
                            if super::preview::media_type(&file.relative_path, text.as_bytes())
                                == "text/html"
                            {
                                this.apply(Update::HtmlPreview(text), cx);
                            } else {
                                this.apply(
                                    Update::TextPreview(file.relative_path.clone(), text, true),
                                    cx,
                                );
                            }
                        },
                    )),
                )
                .child(textarea(&self.file_text).h_64().disabled(self.busy))
                .child(
                    button("mark-dirty", "Mark buffer as edited")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.dirty_presence(true, cx))),
                )
                .child(
                    button("save-file", "Save file")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let file = this.file.clone().unwrap();
                            let expected_hash = this.file_hash.clone();
                            let bytes = this.file_text.read(cx).value().as_bytes().to_vec();
                            this.run(
                                async move {
                                    let artifact = host
                                        .upload(
                                            p::ResourceRef {
                                                resource: Some(p::resource_ref::Resource::File(
                                                    file.clone(),
                                                )),
                                            },
                                            "file-save",
                                            "application/octet-stream",
                                            bytes,
                                        )
                                        .await?;
                                    let save_file = file.clone();
                                    host.mutate(None, move |channel, command| async move {
                                        p::files_service_client::FilesServiceClient::new(channel)
                                            .save_file(p::SaveFileRequest {
                                                command: Some(command),
                                                file: Some(save_file),
                                                expected_hash,
                                                content: Some(artifact),
                                            })
                                            .await
                                    })
                                    .await?;
                                    let (bytes, hash) = host.read_file(file.clone()).await?;
                                    let text = String::from_utf8(bytes)
                                        .map_err(|_| "Saved file is not UTF-8.")?;
                                    let base_hash = hash.clone();
                                    let dirty_file = file.clone();
                                    host.mutate(None, move |channel, command| async move {
                                        p::files_service_client::FilesServiceClient::new(channel)
                                            .set_dirty_presence(p::DirtyPresenceRequest {
                                                command: Some(command),
                                                file: Some(dirty_file),
                                                base_hash,
                                                dirty: false,
                                            })
                                            .await
                                    })
                                    .await?;
                                    host.save_view(file_view_key(&file), String::new())?;
                                    Ok(Update::File(file, hash, text, false))
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    button("discard-presence", "Discard file edits")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let file = this.file.clone().unwrap();
                            this.apply(
                                Update::Confirm(
                                    "Discard file edits?".into(),
                                    "Reload the host file and remove this device’s saved draft."
                                        .into(),
                                    Box::pin(async move {
                                        let (bytes, hash) = host.read_file(file.clone()).await?;
                                        let text = String::from_utf8(bytes)
                                            .map_err(|_| "File is not UTF-8.")?;
                                        let target = file.clone();
                                        let base_hash = hash.clone();
                                        host.mutate(None, move |channel, command| async move {
                                            p::files_service_client::FilesServiceClient::new(
                                                channel,
                                            )
                                            .set_dirty_presence(p::DirtyPresenceRequest {
                                                command: Some(command),
                                                file: Some(target),
                                                base_hash,
                                                dirty: false,
                                            })
                                            .await
                                        })
                                        .await?;
                                        host.save_view(file_view_key(&file), String::new())?;
                                        Ok(Update::File(file, hash, text, false))
                                    }),
                                ),
                                cx,
                            );
                        })),
                );
        }
        body.into_any_element()
    }
    pub(super) fn dirty_presence(&mut self, dirty: bool, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let file = self.file.clone();
        let base_hash = self.file_hash.clone();
        self.run(
            async move {
                host.mutate(None, move |channel, command| async move {
                    p::files_service_client::FilesServiceClient::new(channel)
                        .set_dirty_presence(p::DirtyPresenceRequest {
                            command: Some(command),
                            file,
                            base_hash,
                            dirty,
                        })
                        .await
                })
                .await?;
                Ok(Update::Done(
                    if dirty {
                        "Host knows this buffer is dirty."
                    } else {
                        "Dirty marker cleared."
                    }
                    .into(),
                ))
            },
            cx,
        );
    }
    pub(super) fn load_git(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(workspace)) = (self.host.clone(), self.workspace.clone()) else {
            return;
        };
        if !self.ready() {
            return;
        }
        self.run(
            async move {
                Ok(Update::Git(
                    p::git_service_client::GitServiceClient::new(host.channel()?)
                        .get_status(p::WorkspaceRequest {
                            workspace: Some(workspace),
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                ))
            },
            cx,
        );
    }
    pub(super) fn render_git(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.workspace.is_none() {
            return div()
                .child("Select a workspace in Projects.")
                .into_any_element();
        }
        let mut body = column().gap_3().child(
            button("refresh-git", "Refresh changes")
                .disabled(!self.ready())
                .on_click(cx.listener(|this, _, _, cx| this.load_git(cx))),
        );
        if let Some(status) = self.git.clone() {
            body = body.child(format!("{} · {}", status.branch, status.head_oid));
            for file in status.files {
                body = body.child(div().child(format!(
                    "{} · {} {}",
                    file.path, file.index_status, file.worktree_status
                )));
                for (staged, label) in [(false, "Working diff"), (true, "Staged diff")] {
                    let path = file.path.clone();
                    body = body.child(
                        button(SharedString::from(format!("diff:{staged}:{path}")), label)
                            .disabled(!self.ready())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.read_git_diff(path.clone(), staged, None, cx)
                            })),
                    );
                }
                for (operation, label) in [(1, "Stage"), (2, "Unstage"), (3, "Discard…")] {
                    let path = file.path.clone();
                    body=body.child(button(SharedString::from(format!("git:{operation}:{path}")),label).disabled(!self.ready()).on_click(cx.listener(move|this,_,_,cx|{let host=this.host.clone().unwrap();let workspace=this.workspace.clone();let paths=vec![path.clone()];this.run(async move{let mut client=p::git_service_client::GitServiceClient::new(host.channel()?);let plan=client.preflight(p::GitPreflightRequest{workspace:workspace.clone(),intent:Some(p::git_preflight_request::Intent::Paths(p::GitPathIntent{operation,paths:paths.clone()}))}).await.map_err(rpc_error)?.into_inner();if !plan.can_execute{return Err(plan.reason);}
if operation==3{let detail=plan.impacts.join("\n");let expected=plan.expected;return Ok(Update::Confirm("Discard these changes?".into(),detail,Box::pin(async move{host.mutate(None,move|channel,command|async move{p::git_service_client::GitServiceClient::new(channel).discard(p::GitPathsRequest{command:Some(command),workspace,paths,expected}).await}).await?;Ok(Update::Done("Changes discarded. Refresh changes.".into()))})));}let expected=plan.expected;host.mutate(None,move|channel,command|async move{let mut client=p::git_service_client::GitServiceClient::new(channel);let request=p::GitPathsRequest{command:Some(command),workspace,paths,expected};if operation==1{client.stage(request).await}else{client.unstage(request).await}}).await?;Ok(Update::Done("Git operation accepted. Refresh changes.".into()))},cx);})));
                }
            }
        }
        body = body
            .child(self.field("commit", "Commit message"))
            .child(
                button("commit", "Commit")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| this.git_operation(0, cx))),
            )
            .child(
                checkbox("git-sign", self.git_sign, "Sign commit").on_click(cx.listener(
                    |this, value, _, cx| {
                        this.git_sign = *value;
                        cx.notify();
                    },
                )),
            )
            .child(self.field("remote", "Git remote (empty uses origin)"))
            .child(self.field("upstream_branch", "Upstream branch (optional)"))
            .child(
                button("pull", "Pull")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| this.git_operation(1, cx))),
            )
            .child(
                button("push", "Push")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| this.git_operation(2, cx))),
            );
        body = body.child(
            row()
                .gap_2()
                .child(
                    button("git-history", "Load history")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.read_git_history(cx))),
                )
                .child(
                    button("git-refs", "Load branches and tags")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let workspace = this.workspace.clone();
                            this.run(
                                async move {
                                    Ok(Update::GitRefs(
                                        p::git_service_client::GitServiceClient::new(
                                            host.channel()?,
                                        )
                                        .list_refs(p::WorkspaceRequest { workspace })
                                        .await
                                        .map_err(rpc_error)?
                                        .into_inner(),
                                    ))
                                },
                                cx,
                            );
                        })),
                ),
        );
        if let Some(history) = self.git_history.clone() {
            for commit in history.commits {
                let oid = commit.oid.clone();
                body = body.child(
                    button(
                        SharedString::from(format!("commit:{oid}")),
                        format!("{} · {} · {}", commit.title, commit.author, oid),
                    )
                    .disabled(!self.ready())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.read_git_diff(String::new(), false, Some(oid.clone()), cx)
                    })),
                );
            }
        }
        if let Some(refs) = &self.git_refs {
            for reference in &refs.refs {
                body = body.child(format!(
                    "{} · {} · {}",
                    reference.kind, reference.name, reference.oid
                ));
            }
        }
        if let Some(diff) = &self.git_diff {
            body = body.child(
                div()
                    .font_family(t::terminal_font().family)
                    .child(diff.clone()),
            );
        }
        body = body
            .child(self.field("branch", "Worktree branch"))
            .child(self.field("worktree_path", "Worktree path"))
            .child(self.field("start_point", "Start point (optional)"))
            .child(self.field("target_branch", "Merge target branch"));
        for (index, label) in [
            (0, "Delete branch"),
            (1, "Discard worktree changes"),
            (2, "Allow deleting unmerged branch"),
            (3, "Acknowledge disconnected devices with dirty buffers"),
        ] {
            body = body.child(
                checkbox(
                    SharedString::from(format!("worktree-flag:{index}")),
                    self.worktree_flags[index],
                    label,
                )
                .on_click(cx.listener(move |this, value, _, cx| {
                    this.worktree_flags[index] = *value;
                    cx.notify();
                })),
            );
        }
        for (action, label) in [
            (1, "Create worktree…"),
            (2, "Remove worktree…"),
            (3, "Merge worktree…"),
            (4, "Stop worktree processes…"),
        ] {
            body = body.child(
                button(SharedString::from(format!("worktree:{action}")), label)
                    .disabled(!self.ready())
                    .on_click(cx.listener(move |this, _, _, cx| this.change_worktree(action, cx))),
            );
        }
        body.into_any_element()
    }
    fn read_git_history(&mut self, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let workspace = self.workspace.clone();
        let before_oid = self
            .git_history
            .as_ref()
            .and_then(|h| h.next_before_oid.clone());
        self.run(
            async move {
                Ok(Update::GitHistory(
                    p::git_service_client::GitServiceClient::new(host.channel()?)
                        .read_history(p::GitHistoryRequest {
                            workspace,
                            before_oid,
                            limit: 50,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                ))
            },
            cx,
        );
    }
    fn read_git_diff(
        &mut self,
        path: String,
        staged: bool,
        commit: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let host = self.host.clone().unwrap();
        let workspace = self.workspace.clone();
        self.run(
            async move {
                let mut stream = p::git_service_client::GitServiceClient::new(host.channel()?)
                    .read_diff(p::DiffRequest {
                        workspace,
                        path,
                        staged,
                        commit,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                let mut text = String::new();
                let mut complete = false;
                while let Some(chunk) = stream.message().await.map_err(rpc_error)? {
                    if complete {
                        return Err("Unexpected data after completed diff.".into());
                    }
                    if chunk.binary {
                        text.push_str("Binary file changed\n");
                    }
                    for line in chunk.lines {
                        text.push_str(&format!("{} {}\n", line.kind, line.text));
                    }
                    if text.len() > 8 * 1024 * 1024 {
                        return Err("Diff exceeds the display limit.".into());
                    }
                    if chunk.truncated {
                        text.push_str("\nDiff truncated by host.\n");
                    }
                    complete = chunk.complete;
                }
                if !complete {
                    return Err("Diff was interrupted; retry to load a complete result.".into());
                }
                Ok(Update::GitDiff(text))
            },
            cx,
        );
    }
    fn change_worktree(&mut self, action: i32, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let workspace = self.workspace.clone();
        let intent = p::GitWorktreeIntent {
            action,
            branch: self.value("branch", cx),
            path: nonempty(self.value("worktree_path", cx)),
            start_point: nonempty(self.value("start_point", cx)),
            target_branch: nonempty(self.value("target_branch", cx)),
            delete_branch: self.worktree_flags[0],
            discard_changes: self.worktree_flags[1],
            delete_unmerged_branch: self.worktree_flags[2],
            acknowledge_offline_dirty: self.worktree_flags[3],
        };
        self.run(
            async move {
                let plan = p::git_service_client::GitServiceClient::new(host.channel()?)
                    .preflight(p::GitPreflightRequest {
                        workspace: workspace.clone(),
                        intent: Some(p::git_preflight_request::Intent::Worktree(intent.clone())),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if !plan.can_execute {
                    return Err(plan.reason);
                }
                Ok(Update::Confirm(
                    "Apply worktree operation?".into(),
                    plan.impacts.join("\n"),
                    Box::pin(async move {
                        host.mutate(None, move |channel, command| async move {
                            p::git_service_client::GitServiceClient::new(channel)
                                .change_worktree(p::WorktreeRequest {
                                    command: Some(command),
                                    workspace,
                                    action: intent.action,
                                    branch: intent.branch,
                                    path: intent.path,
                                    expected: plan.expected,
                                    start_point: intent.start_point,
                                    target_branch: intent.target_branch,
                                    delete_branch: intent.delete_branch,
                                    discard_changes: intent.discard_changes,
                                    delete_unmerged_branch: intent.delete_unmerged_branch,
                                    acknowledge_offline_dirty: intent.acknowledge_offline_dirty,
                                })
                                .await
                        })
                        .await?;
                        Ok(Update::Done(
                            "Worktree operation completed. Refresh its status.".into(),
                        ))
                    }),
                ))
            },
            cx,
        );
    }
    fn git_operation(&mut self, operation: u8, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let workspace = self.workspace.clone();
        let message = self.value("commit", cx);
        let sign = self.git_sign;
        let upstream_branch = nonempty(self.value("upstream_branch", cx));
        let remote = nonempty(self.value("remote", cx)).unwrap_or("origin".into());
        self.run(
            async move {
                let intent = if operation == 0 {
                    p::git_preflight_request::Intent::Commit(p::GitCommitIntent {
                        message: message.clone(),
                        sign,
                    })
                } else {
                    p::git_preflight_request::Intent::Network(p::GitNetworkIntent {
                        action: operation as i32,
                        remote: remote.clone(),
                        upstream_branch: upstream_branch.clone(),
                    })
                };
                let plan = p::git_service_client::GitServiceClient::new(host.channel()?)
                    .preflight(p::GitPreflightRequest {
                        workspace: workspace.clone(),
                        intent: Some(intent),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if !plan.can_execute {
                    return Err(plan.reason);
                }
                host.mutate(None, move |channel, command| async move {
                    let mut client = p::git_service_client::GitServiceClient::new(channel);
                    if operation == 0 {
                        client
                            .commit(p::CommitRequest {
                                command: Some(command),
                                workspace,
                                message,
                                expected: plan.expected,
                                sign,
                            })
                            .await
                    } else {
                        let request = p::GitNetworkRequest {
                            command: Some(command),
                            workspace,
                            remote,
                            expected: plan.expected,
                            upstream_branch: upstream_branch.clone(),
                        };
                        if operation == 1 {
                            client.pull(request).await
                        } else {
                            client.push(request).await
                        }
                    }
                })
                .await?;
                Ok(Update::Done(
                    "Git operation accepted. Refresh its status before retrying.".into(),
                ))
            },
            cx,
        );
    }
}

fn file_view_key(file: &p::FileRef) -> String {
    format!(
        "file:{}:{}",
        file.workspace
            .as_ref()
            .map(|w| w.workspace_id.as_str())
            .unwrap_or_default(),
        file.relative_path
    )
}
