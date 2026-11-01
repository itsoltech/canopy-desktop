//! Files and Changes for the selected checkout, laid out like the local tool pages.
use super::*;
use canopy_desktop::remote::client::rpc_error;
use chrome::*;
use gpui_kit::component::{IconName, button::ButtonVariants, scroll::ScrollableElement};

/// Width of the browsing column beside an open file or diff in wide layouts.
const BROWSER_WIDTH: f32 = 300.;
const CHANGES_WIDTH: f32 = 340.;

fn parent_folder(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default()
}
fn join_path(folder: &str, name: &str) -> String {
    let folder = folder.trim_matches('/');
    let name = name.trim_matches('/');
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}
/// Change letter shared with the local Changes list (untracked reads as added).
fn change_kind(status: &str) -> Option<&'static str> {
    match status.trim() {
        "" => None,
        "?" | "A" => Some("A"),
        "D" => Some("D"),
        "R" => Some("R"),
        "U" => Some("U"),
        "T" => Some("T"),
        _ => Some("M"),
    }
}
/// File-tree row geometry: chevron/file slot, folder icon, label, metadata.
fn file_row(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    detail: Option<String>,
    folder: bool,
    selected: bool,
) -> Stateful<Div> {
    let name: SharedString = name.into();
    row()
        .id(id)
        .keyboard(if folder {
            format!("Open folder {name}")
        } else {
            format!("Open {name}")
        })
        .relative()
        .w_full()
        .h(px(t::ROW))
        .flex_shrink_0()
        .gap(px(8.))
        .pl(px(files::TREE_INSET))
        .pr(px(8.))
        .cursor_pointer()
        .rounded(px(3.))
        .when(selected, |row| row.bg(t::selected()))
        .when(!selected, |row| row.hover(|style| style.bg(t::hover())))
        .child(
            icon(if folder {
                IconName::ChevronRight
            } else {
                IconName::File
            })
            .size(px(12.)),
        )
        .children(folder.then(|| icon(IconName::Folder)))
        .child(div().flex_1().min_w_0().truncate().child(name))
        .children(detail.map(|detail| {
            div()
                .max_w(px(160.))
                .truncate()
                .text_size(px(10.))
                .text_color(t::faint())
                .child(detail)
        }))
}
fn diff_lines(diff: &str) -> Div {
    column().w_full().children(diff.lines().map(|line| {
        let (kind, text) = line.split_once(' ').unwrap_or((line, ""));
        let mark = match kind {
            "+" | "add" | "addition" => '+',
            "-" | "delete" | "deletion" | "remove" => '-',
            "@" | "@@" | "hunk" | "H" => 'H',
            _ => ' ',
        };
        let plain = !matches!(mark, '+' | '-' | 'H') && kind.len() > 1;
        let text = if plain { line } else { text };
        row()
            .w_full()
            .min_h(px(20.))
            .px(px(8.))
            .gap(px(8.))
            .bg(t::diff_line_background(mark))
            .font_family(t::MONO)
            .text_size(px(12.))
            .text_color(if mark == 'H' { t::muted() } else { t::text() })
            .child(
                div()
                    .w(px(12.))
                    .flex_shrink_0()
                    .text_color(match mark {
                        '+' => t::green(),
                        '-' => t::red(),
                        _ => t::faint(),
                    })
                    .child(if matches!(mark, '+' | '-') {
                        mark.to_string()
                    } else {
                        String::new()
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .child(text.replace('\t', "    ")),
            )
    }))
}

impl RemoteWindow {
    pub(super) fn save_file(&mut self, cx: &mut Context<Self>) {
        if !self.ready() || self.file.is_none() {
            return;
        }
        let host = self.host.clone().unwrap();
        let file = self.file.clone().unwrap();
        let expected_hash = self.file_hash.clone();
        let bytes = self.file_text.read(cx).value().as_bytes().to_vec();
        self.run_in(
            "save",
            async move {
                let artifact = host
                    .upload(
                        canopy_desktop::remote::client::UploadTarget::File(file.clone()),
                        "file_save",
                        "application/octet-stream",
                        bytes,
                    )
                    .await?;
                host.save_file(file.clone(), expected_hash, artifact)
                    .await?;
                let (bytes, hash) = host.read_file(file.clone()).await?;
                let text = String::from_utf8(bytes).map_err(|_| "Saved file is not UTF-8.")?;
                let base_hash = hash.clone();
                host.set_file_presence(file.clone(), base_hash, false)
                    .await?;
                host.save_view(file_view_key(&file), String::new())?;
                Ok(Update::File(file, hash, text, false))
            },
            cx,
        );
    }

    pub(super) fn open_remote_file(&mut self, file: p::FileRef, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.run_in(
            "files",
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
        self.run_in(
            "files",
            async move {
                let append = token.is_some();
                let result = host
                    .list_directory(
                        p::FileRef {
                            target: project_target(&workspace),
                            relative_path,
                        },
                        token,
                    )
                    .await?;
                Ok(if append {
                    Update::DirectoryPage(result)
                } else {
                    Update::Directory(result)
                })
            },
            cx,
        );
    }
    pub(super) fn search_files(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        if !self.ready() || self.workspace.is_none() {
            return;
        }
        let workspace = self.workspace.clone().unwrap();
        let query = self.value("query", cx);
        self.run_in(
            "files",
            async move {
                Ok(Update::Directory(
                    host.search_files(
                        project_target(&workspace).ok_or("Project unavailable.")?,
                        query,
                        200,
                    )
                    .await?,
                ))
            },
            cx,
        );
    }
    fn open_folder(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.fields["path"].update(cx, |s, cx| s.set_value(path, window, cx));
        self.save_layout(cx);
        self.load_directory(cx);
    }
    /// Asks for a name, creates it in the shown folder and lists that folder again.
    fn create_entry(&mut self, directory: bool, cx: &mut Context<Self>) {
        let folder = self.value("path", cx);
        let label = format!(
            "Name in {}",
            if folder.is_empty() {
                "the checkout root".to_owned()
            } else {
                folder.clone()
            }
        );
        self.open_prompt(
            if directory { "New folder" } else { "New file" },
            label,
            String::new(),
            "Create",
            move |this, name, cx| {
                if name.is_empty() {
                    return Err("Enter a name.".into());
                }
                let (Some(host), Some(workspace)) = (this.host.clone(), this.workspace.clone())
                else {
                    return Err("Select a checkout first.".into());
                };
                if !this.ready() {
                    return Err("Connect to the computer first.".into());
                }
                let file = p::FileRef {
                    target: project_target(&workspace),
                    relative_path: join_path(&folder, &name),
                };
                let listed = folder.clone();
                this.run_in(
                    "files",
                    async move {
                        host.create_file(file, directory, None).await?;
                        let listing = host
                            .list_directory(
                                p::FileRef {
                                    target: project_target(&workspace),
                                    relative_path: listed,
                                },
                                None,
                            )
                            .await?;
                        Ok(Update::Directory(listing))
                    },
                    cx,
                );
                Ok(())
            },
            cx,
        );
    }
    fn discard_file_edits(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(file)) = (self.host.clone(), self.file.clone()) else {
            return;
        };
        self.scope = Some("editor");
        self.apply(
            Update::Confirm(
                "Discard file edits?".into(),
                format!(
                    "{} is reloaded from the other computer and this device’s draft is removed.",
                    file.relative_path
                ),
                Box::pin(async move {
                    let (bytes, hash) = host.read_file(file.clone()).await?;
                    let text = String::from_utf8(bytes).map_err(|_| "File is not UTF-8.")?;
                    let base_hash = hash.clone();
                    host.set_file_presence(file.clone(), base_hash, false)
                        .await?;
                    host.save_view(file_view_key(&file), String::new())?;
                    Ok(Update::File(file, hash, text, false))
                }),
            ),
            cx,
        );
    }
    fn preview_file(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.file.as_ref() else {
            return;
        };
        let text = self.file_text.read(cx).value().to_string();
        if super::preview::media_type(&file.relative_path, text.as_bytes()) == "text/html" {
            self.apply(Update::HtmlPreview(text), cx);
        } else {
            let title = file.relative_path.clone();
            self.apply(Update::TextPreview(title, text, true), cx);
        }
    }
    fn no_checkout(&self, page: &str, cx: &mut Context<Self>) -> AnyElement {
        empty(
            "No checkout selected",
            format!("Choose a checkout in Projects to see its {page}."),
        )
        .children((!self.embedded).then(|| {
            button("open-projects", "Open Projects")
                .on_click(cx.listener(|this, _, _, cx| this.show_page(Page::Catalog, cx)))
        }))
        .into_any_element()
    }
    fn file_browser(&mut self, cx: &mut Context<Self>) -> Div {
        let ready = self.ready();
        let path = self.value("path", cx);
        let loading_files = self.pending == Some("files");
        let toolbar = row()
            .h(px(t::ROW))
            .flex_shrink_0()
            .gap(px(4.))
            .child(
                icon_button("files-up", IconName::ArrowUp, "Parent folder")
                    .disabled(!ready || path.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        let parent = parent_folder(&this.value("path", cx));
                        this.open_folder(parent, window, cx);
                    })),
            )
            .child(input(&self.fields["path"]).flex_1().min_w_0())
            .child(
                icon_button("list-files", IconName::RotateCw, "Reload folder")
                    .loading(loading_files)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.load_directory(cx))),
            )
            .child(
                icon_button("create-file:false", IconName::Plus, "New file…")
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.create_entry(false, cx))),
            )
            .child(
                icon_button_base("create-file:true", "New folder…")
                    .icon(icon(IconName::FolderOpen))
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.create_entry(true, cx))),
            );
        let search = row()
            .flex_shrink_0()
            .gap(px(4.))
            .child(input(&self.fields["query"]).flex_1().min_w_0())
            .child(
                icon_button("search-files", IconName::Search, "Search files")
                    .disabled(!ready || self.value("query", cx).trim().is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.search_files(cx))),
            );
        let mut list = column().w_full().flex_shrink_0();
        match self.directory.clone() {
            None if loading_files => list = list.child(loading("Loading files…")),
            None if !ready => {
                list = list.child(
                    div()
                        .py(px(8.))
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .child("Connect to browse files."),
                )
            }
            None => {}
            Some(directory) => {
                if directory.entries.is_empty() {
                    list = list.child(
                        div()
                            .py(px(8.))
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .child("This folder is empty."),
                    );
                }
                let selected = self.file.as_ref().map(|f| f.relative_path.clone());
                for entry in directory.entries {
                    let entry_path = entry.relative_path.clone();
                    let parent = parent_folder(&entry_path);
                    let detail =
                        (parent != path.trim_matches('/') && !parent.is_empty()).then_some(parent);
                    let folder = entry.directory;
                    list = list.child(
                        file_row(
                            SharedString::from(format!("file:{entry_path}")),
                            entry.name.clone(),
                            detail,
                            folder,
                            selected.as_deref() == Some(entry_path.as_str()),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if !this.ready() {
                                    return;
                                }
                                if folder {
                                    this.open_folder(entry_path.clone(), window, cx);
                                } else {
                                    let file = p::FileRef {
                                        target: this.workspace.as_ref().and_then(project_target),
                                        relative_path: entry_path.clone(),
                                    };
                                    this.open_remote_file(file, cx);
                                }
                            },
                        )),
                    );
                }
                if let Some(token) = directory.next_page_token.clone() {
                    list = list.child(
                        row().pt(px(4.)).child(
                            button("directory-more", "Load more")
                                .ghost()
                                .border_0()
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.load_directory_page(Some(token.clone()), cx)
                                })),
                        ),
                    );
                }
                if let Some(warning) = directory.warning {
                    list = list.child(div().pt(px(8.)).child(hint(warning)));
                } else if directory.incomplete {
                    list = list.child(
                        div()
                            .pt(px(8.))
                            .child(hint("Some entries are missing. Refine the search.")),
                    );
                }
            }
        }
        column()
            .min_h_0()
            .gap(px(8.))
            .child(toolbar)
            .child(search)
            .children(self.notice("files"))
            .child(
                column()
                    .id("remote-files-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(list),
            )
    }
    fn file_editor(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let file = self.file.clone()?;
        let now = Instant::now();
        let ready = self.ready();
        let name = file
            .relative_path
            .rsplit('/')
            .next()
            .unwrap_or(&file.relative_path)
            .to_owned();
        let header = row()
            .h(px(32.))
            .flex_shrink_0()
            .gap(px(8.))
            .child(icon(IconName::File))
            .child(row_text(name, file.relative_path.clone()))
            .children(self.file_dirty.then(|| badge("Edited")))
            .child(
                button("preview-text-file", "Preview…")
                    .ghost()
                    .border_0()
                    .on_click(cx.listener(|this, _, _, cx| this.preview_file(cx))),
            )
            .child(
                button("discard-presence", "Discard edits…")
                    .ghost()
                    .border_0()
                    .text_color(t::red())
                    .disabled(!ready || !self.file_dirty)
                    .on_click(cx.listener(|this, _, _, cx| this.discard_file_edits(cx))),
            )
            .child(
                primary_button("save-file", "Save")
                    .loading(self.pending == Some("save"))
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.save_file(cx))),
            );
        let diagnostics = stacked_setting(
            "Mark as edited on the other computer",
            "Tells the other computer this device has unsaved changes, so closing the project warns about them.",
            true,
            row().child(
                button("mark-dirty", "Mark as edited")
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.dirty_presence(true, cx))),
            ),
        );
        Some(
            column()
                .flex_1()
                .min_h_0()
                .gap(px(8.))
                .child(header)
                .children(self.notice("save"))
                .children(self.notice("editor"))
                .child(
                    textarea(&self.file_text)
                        .flex_1()
                        .min_h(px(160.))
                        .font_family(t::MONO)
                        .disabled(self.busy),
                )
                .child(
                    column()
                        .flex_shrink_0()
                        .child(
                            self.file_diagnostics
                                .header("file-diagnostics", "ADVANCED", now, cx)
                                .on_hover(cx.listener(|this, hovered, _, cx| {
                                    this.file_diagnostics.hovered = *hovered;
                                    cx.notify();
                                }))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.file_diagnostics.toggle(Instant::now(), cx);
                                    cx.notify();
                                })),
                        )
                        .child(self.file_diagnostics.measured_body(
                            "file-diagnostics-body",
                            diagnostics,
                            now,
                        )),
                ),
        )
    }
    pub(super) fn render_files(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.workspace.is_none() {
            return self.no_checkout("files", cx);
        }
        let browser = self.file_browser(cx);
        let editor = self.file_editor(cx);
        if self.compact_tools {
            let open = editor.is_some();
            return column()
                .flex_1()
                .min_h_0()
                .gap(px(12.))
                .child(if open {
                    browser.h(px(220.)).flex_shrink_0()
                } else {
                    browser.flex_1()
                })
                .children(
                    editor.map(|editor| editor.pt(px(12.)).border_t_1().border_color(t::border())),
                )
                .into_any_element();
        }
        row()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .gap(px(16.))
            .child(browser.w(px(BROWSER_WIDTH)).flex_shrink_0().h_full())
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pl(px(16.))
                    .border_l_1()
                    .border_color(t::border())
                    .child(editor.unwrap_or_else(|| {
                        empty("No file open", "Choose a file to view or edit it.")
                    })),
            )
            .into_any_element()
    }
    pub(super) fn dirty_presence(&mut self, dirty: bool, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let file = self.file.clone();
        let base_hash = self.file_hash.clone();
        self.run_in(
            "editor",
            async move {
                host.set_file_presence(file.ok_or("File is unavailable.")?, base_hash, dirty)
                    .await?;
                Ok(Update::Done(
                    if dirty {
                        "The other computer knows this file has unsaved edits."
                    } else {
                        "Edit marker cleared."
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
        self.run_in(
            "git-status",
            async move {
                Ok(Update::Git(
                    api::git_service_client::GitServiceClient::new(host.channel()?)
                        .get_status(api::GetStatusRequest {
                            project: workspace.project,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                ))
            },
            cx,
        );
    }
    /// Stage (1), unstage (2) or discard (3) one path after a typed preflight.
    fn change_paths(&mut self, operation: i32, path: String, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        let paths = vec![path];
        self.run_in(
            "git",
            async move {
                let project = Some(
                    workspace
                        .ok_or("Project is unavailable.")?
                        .project
                        .ok_or("Project is unavailable.")?,
                );
                let mut client = api::git_service_client::GitServiceClient::new(host.channel()?);
                let plan = client
                    .preflight(api::PreflightRequest {
                        project: project.clone(),
                        intent: Some(api::preflight_request::Intent::Paths(api::GitPathIntent {
                            operation,
                            paths: paths.clone(),
                        })),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if !plan.can_execute {
                    return Err(plan.reason);
                }
                let expected = plan.expected;
                if operation == 3 {
                    let detail = format!(
                        "{}\nThis cannot be undone.",
                        plan.impacts
                            .iter()
                            .map(|impact| format!("• {impact}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    return Ok(Update::Confirm(
                        "Discard these changes?".into(),
                        detail,
                        Box::pin(async move {
                            host.git_discard(api::DiscardRequest {
                                command: None,
                                project,
                                paths,
                                expected,
                            })
                            .await?;
                            Ok(Update::Done(
                                "Discard sent. Refresh to see the result.".into(),
                            ))
                        }),
                    ));
                }
                if operation == 1 {
                    host.git_stage(api::StageRequest {
                        command: None,
                        project,
                        paths,
                        expected,
                    })
                    .await?;
                } else {
                    host.git_unstage(api::UnstageRequest {
                        command: None,
                        project,
                        paths,
                        expected,
                    })
                    .await?;
                }
                Ok(Update::Done(
                    if operation == 1 {
                        "Stage sent. Refresh to see the result."
                    } else {
                        "Unstage sent. Refresh to see the result."
                    }
                    .into(),
                ))
            },
            cx,
        );
    }
    fn change_rows(&self, status: &api::GitStatus, staged: bool, cx: &mut Context<Self>) -> Div {
        let ready = self.ready();
        let files = status
            .files
            .iter()
            .filter_map(|file| {
                let kind = if staged {
                    change_kind(&file.index_status).filter(|_| file.index_status.trim() != "?")
                } else {
                    change_kind(&file.worktree_status)
                };
                kind.map(|kind| (file, kind))
            })
            .collect::<Vec<_>>();
        let mut group = column().child(git::change_group(staged, files.len()));
        for (file, kind) in files {
            let key = SharedString::from(format!("{staged}:{}", file.path));
            let hover = key.clone();
            let diff_path = file.path.clone();
            let stage_path = file.path.clone();
            let discard_path = file.path.clone();
            group = group.child(
                git::change_file_row(
                    SharedString::from(format!("git-row:{key}")),
                    kind,
                    self.hovered.as_ref() == Some(&key),
                )
                .on_hover(cx.listener(move |this, over, _, cx| {
                    if *over {
                        this.hovered = Some(hover.clone());
                    } else if this.hovered.as_ref() == Some(&hover) {
                        this.hovered = None;
                    }
                    cx.notify();
                }))
                .child(
                    list_button(
                        SharedString::from(format!("diff:{staged}:{}", file.path)),
                        file.path.clone(),
                        self.hovered.as_ref() == Some(&key),
                        cx,
                    )
                    .px_0()
                    .flex_1()
                    .min_w_0()
                    .disabled(!ready)
                    .tooltip(
                        file.previous_path
                            .as_ref()
                            .map(|old| format!("{old} → {}", file.path))
                            .unwrap_or_else(|| file.path.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.read_git_diff(diff_path.clone(), staged, None, cx)
                    })),
                )
                .child(
                    git::change_action(
                        SharedString::from(format!(
                            "git:{}:{}",
                            if staged { 2 } else { 1 },
                            file.path
                        )),
                        if staged {
                            IconName::Minus
                        } else {
                            IconName::Plus
                        },
                        if staged { "Unstage file" } else { "Stage file" },
                    )
                    .disabled(!ready)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_paths(if staged { 2 } else { 1 }, stage_path.clone(), cx)
                    })),
                )
                .children((!staged).then(|| {
                    git::change_action(
                        SharedString::from(format!("git:3:{}", file.path)),
                        IconName::Close,
                        "Discard changes…",
                    )
                    .disabled(!ready)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_paths(3, discard_path.clone(), cx)
                    }))
                })),
            );
        }
        group
    }
    fn diff_view(&self, cx: &mut Context<Self>) -> Option<Div> {
        let diff = self.git_diff.as_ref()?;
        Some(
            column()
                .flex_1()
                .min_h(px(160.))
                .border_1()
                .border_color(t::border())
                .rounded(px(4.))
                .child(
                    row()
                        .h(px(t::ROW))
                        .flex_shrink_0()
                        .px(px(8.))
                        .gap(px(8.))
                        .border_b_1()
                        .border_color(t::border())
                        .child(caption("DIFF").flex_1())
                        .child(
                            icon_button("close-diff", IconName::Close, "Close diff").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.git_diff = None;
                                    cx.notify();
                                }),
                            ),
                        ),
                )
                .child(
                    div()
                        .id("remote-diff-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_scroll()
                        .child(diff_lines(diff)),
                ),
        )
    }
    fn commit_form(&self, cx: &mut Context<Self>) -> Div {
        let ready = self.ready();
        git::commit_form()
            .child(input(&self.fields["commit"]).w_full().disabled(!ready))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        checkbox("git-sign", self.git_sign, "Sign commit")
                            .label("Sign commit")
                            .on_click(cx.listener(|this, value, _, cx| {
                                this.git_sign = *value;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        primary_button("commit", "Commit")
                            .loading(self.pending == Some("commit"))
                            .disabled(!ready || self.value("commit", cx).trim().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.git_operation(0, cx))),
                    ),
            )
            .children(self.notice("commit"))
    }
    fn transfer(&self, cx: &mut Context<Self>) -> Div {
        let now = Instant::now();
        let ready = self.ready();
        let remote = column()
            .gap(px(12.))
            .pb(px(8.))
            .child(form_field(
                "Remote",
                "Empty uses origin.",
                input(&self.fields["remote"]).w_full(),
            ))
            .child(form_field(
                "Upstream branch",
                "Optional. Set when the branch has no upstream yet.",
                input(&self.fields["upstream_branch"]).w_full(),
            ));
        column()
            .flex_shrink_0()
            .gap(px(8.))
            .py(px(12.))
            .border_t_1()
            .border_color(t::border())
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        button("pull", "↓ Pull")
                            .flex_1()
                            .loading(self.pending == Some("pull"))
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.git_operation(1, cx))),
                    )
                    .child(
                        button("push", "↑ Push")
                            .flex_1()
                            .loading(self.pending == Some("push"))
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.git_operation(2, cx))),
                    ),
            )
            .children(self.notice("pull"))
            .children(self.notice("push"))
            .child(
                self.git_remote
                    .header("git-remote", "REMOTE AND UPSTREAM", now, cx)
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.git_remote.hovered = *hovered;
                        cx.notify();
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.git_remote.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(
                self.git_remote
                    .measured_body("git-remote-body", remote, now),
            )
    }
    fn worktree_section(&self, cx: &mut Context<Self>) -> Div {
        let now = Instant::now();
        let ready = self.ready();
        let fields = [
            form_field("Branch", "", input(&self.fields["branch"]).w_full()),
            form_field(
                "Folder",
                "Optional location for a new worktree.",
                input(&self.fields["worktree_path"]).w_full(),
            ),
            form_field(
                "Start from",
                "Optional commit or branch.",
                input(&self.fields["start_point"]).w_full(),
            ),
            form_field(
                "Merge into",
                "Target branch for Merge.",
                input(&self.fields["target_branch"]).w_full(),
            ),
        ];
        let fields = if self.compact_tools {
            column().gap(px(12.)).children(fields)
        } else {
            field_grid(fields)
        };
        let body = column()
            .gap(px(12.))
            .pb(px(8.))
            .child(fields)
            .child(
                column().gap(px(8.)).children(
                    [
                        (0, "Also delete the branch"),
                        (1, "Discard uncommitted changes"),
                        (2, "Delete the branch even if it is not merged"),
                        (3, "Continue even if offline devices have unsaved edits"),
                    ]
                    .map(|(index, label)| {
                        checkbox(
                            SharedString::from(format!("worktree-flag:{index}")),
                            self.worktree_flags[index],
                            label,
                        )
                        .label(label)
                        .on_click(cx.listener(
                            move |this, value, _, cx| {
                                this.worktree_flags[index] = *value;
                                cx.notify();
                            },
                        ))
                    }),
                ),
            )
            .child(
                row().gap(px(8.)).flex_wrap().children(
                    [
                        (1, "Create…"),
                        (3, "Merge…"),
                        (4, "Stop processes…"),
                        (2, "Remove…"),
                    ]
                    .map(|(action, label)| {
                        button(SharedString::from(format!("worktree:{action}")), label)
                            .when(action == 2, |b| b.text_color(t::red()))
                            .disabled(!ready)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.change_worktree(action, cx)),
                            )
                    }),
                ),
            )
            .children(self.notice("worktree"));
        column()
            .flex_shrink_0()
            .border_t_1()
            .border_color(t::border())
            .child(
                self.worktrees
                    .header("git-worktrees", "WORKTREES", now, cx)
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.worktrees.hovered = *hovered;
                        cx.notify();
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.worktrees.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(
                self.worktrees
                    .measured_body("git-worktrees-body", body, now),
            )
    }
    fn history_list(&self, cx: &mut Context<Self>) -> Div {
        let ready = self.ready();
        let mut list = column().gap(px(2.));
        match &self.git_history {
            None if self.pending == Some("history") => {
                list = list.child(loading("Loading history…"))
            }
            None => {}
            Some(history) => {
                if history.commits.is_empty() {
                    list = list.child(
                        div()
                            .py(px(8.))
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .child("No commits yet."),
                    );
                }
                for commit in &history.commits {
                    let oid = commit.oid.clone();
                    list = list.child(
                        list_row(
                            SharedString::from(format!("commit:{oid}")),
                            format!("Show commit {}", commit.title),
                            false,
                        )
                        .h(px(t::ROW))
                        .child(row_text(commit.title.clone(), commit.author.clone()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_family(t::MONO)
                                .text_size(px(11.))
                                .text_color(t::faint())
                                .child(short_oid(&commit.oid)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.ready() {
                                this.read_git_diff(String::new(), false, Some(oid.clone()), cx)
                            }
                        })),
                    );
                }
                if history.next_before_oid.is_some() {
                    list = list.child(
                        row().pt(px(4.)).child(
                            button("git-history-more", "Load older commits")
                                .ghost()
                                .border_0()
                                .disabled(!ready)
                                .on_click(cx.listener(|this, _, _, cx| this.read_git_history(cx))),
                        ),
                    );
                }
            }
        }
        let mut refs = column()
            .gap(px(2.))
            .child(
                section_title("BRANCHES AND TAGS").children(self.git_refs.is_none().then(|| {
                    button("git-refs", "Show")
                        .ghost()
                        .border_0()
                        .loading(self.pending == Some("refs"))
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(host) = this.host.clone() else {
                                return;
                            };
                            let workspace = this.workspace.clone();
                            this.run_in(
                                "refs",
                                async move {
                                    Ok(Update::GitRefs(
                                        api::git_service_client::GitServiceClient::new(
                                            host.channel()?,
                                        )
                                        .list_refs(api::ListRefsRequest {
                                            project: Some(
                                                workspace
                                                    .ok_or("Project is unavailable.")?
                                                    .project
                                                    .ok_or("Project is unavailable.")?,
                                            ),
                                        })
                                        .await
                                        .map_err(rpc_error)?
                                        .into_inner(),
                                    ))
                                },
                                cx,
                            );
                        }))
                })),
            );
        if let Some(git_refs) = &self.git_refs {
            for reference in &git_refs.refs {
                refs = refs.child(
                    row()
                        .h(px(t::ROW))
                        .px(px(8.))
                        .gap(px(8.))
                        .child(badge(reference.kind.clone()))
                        .child(row_text(reference.name.clone(), ""))
                        .child(
                            div()
                                .font_family(t::MONO)
                                .text_size(px(11.))
                                .text_color(t::faint())
                                .child(short_oid(&reference.oid)),
                        ),
                );
            }
        }
        column()
            .gap(px(16.))
            .child(list)
            .children(self.notice("history"))
            .child(refs.children(self.notice("refs")))
    }
    pub(super) fn render_git(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.workspace.is_none() {
            return self.no_checkout("changes", cx);
        }
        let ready = self.ready();
        let history = self.git_history_tab;
        let tabs = segmented([
            segment("git-files-tab", "Files", !history).on_click(cx.listener(|this, _, _, cx| {
                this.git_history_tab = false;
                cx.notify();
            })),
            segment("git-history", "History", history).on_click(cx.listener(|this, _, _, cx| {
                this.git_history_tab = true;
                if this.git_history.is_none() && this.ready() {
                    this.read_git_history(cx);
                }
                cx.notify();
            })),
        ]);
        let mut main = column().gap(px(8.)).child(tabs);
        if history {
            main = main.child(self.history_list(cx));
        } else {
            let count = self.git.as_ref().map_or(0, |status| status.files.len());
            main = main.child(git::inspector_toolbar(
                format!("{count} change{}", if count == 1 { "" } else { "s" }),
                icon_button("refresh-git", IconName::RotateCw, "Refresh changes")
                    .loading(self.pending == Some("git-status"))
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.load_git(cx))),
            ));
            match self.git.clone() {
                None if self.pending == Some("git-status") => {
                    main = main.child(loading("Loading changes…"))
                }
                None => {
                    main = main.child(div().text_size(px(12.)).text_color(t::muted()).child(
                        if ready {
                            "Refresh to load changes."
                        } else {
                            "Connect to see changes."
                        },
                    ))
                }
                Some(status) => {
                    main = main
                        .child(
                            row()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .text_color(t::secondary())
                                .child(custom_icon("git-branch"))
                                .child(div().min_w_0().truncate().child(
                                    if status.branch.is_empty() {
                                        "Detached HEAD".to_owned()
                                    } else {
                                        status.branch.clone()
                                    },
                                ))
                                .child(
                                    div()
                                        .font_family(t::MONO)
                                        .text_size(px(11.))
                                        .text_color(t::faint())
                                        .child(short_oid(&status.head_oid)),
                                ),
                        )
                        .children(self.notice("git"));
                    if status.files.is_empty() {
                        main = main.child(
                            div()
                                .py(px(8.))
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .child("No changes. The working tree matches the last commit."),
                        );
                    } else {
                        main = main
                            .child(self.change_rows(&status, true, cx))
                            .child(self.change_rows(&status, false, cx));
                    }
                }
            }
            main = main.child(self.commit_form(cx));
        }
        main = main
            .children(self.notice("diff"))
            .child(self.transfer(cx))
            .child(self.worktree_section(cx));
        let diff = self.diff_view(cx);
        if self.compact_tools {
            return column()
                .id("remote-changes-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(
                    column()
                        .flex_shrink_0()
                        .gap(px(8.))
                        .child(main)
                        .children(diff.map(|diff| diff.h(px(320.)).flex_none())),
                )
                .into_any_element();
        }
        row()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .gap(px(16.))
            .child(
                column()
                    .id("remote-changes-scroll")
                    .w(px(CHANGES_WIDTH))
                    .flex_shrink_0()
                    .h_full()
                    .overflow_y_scrollbar()
                    .child(main.flex_shrink_0().pr(px(8.))),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(diff.unwrap_or_else(|| {
                        empty(
                            "No diff selected",
                            "Choose a changed file or a commit to see what changed.",
                        )
                    })),
            )
            .into_any_element()
    }
    fn read_git_history(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        let before_oid = self
            .git_history
            .as_ref()
            .and_then(|h| h.next_before_oid.clone());
        self.run_in(
            "history",
            async move {
                Ok(Update::GitHistory(
                    api::git_service_client::GitServiceClient::new(host.channel()?)
                        .read_history(api::ReadHistoryRequest {
                            project: Some(
                                workspace
                                    .ok_or("Project is unavailable.")?
                                    .project
                                    .ok_or("Project is unavailable.")?,
                            ),
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
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        self.run_in(
            "diff",
            async move {
                let mut stream = api::git_service_client::GitServiceClient::new(host.channel()?)
                    .read_diff(api::ReadDiffRequest {
                        project: Some(
                            workspace
                                .ok_or("Project is unavailable.")?
                                .project
                                .ok_or("Project is unavailable.")?,
                        ),
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
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        let intent = api::GitWorktreeIntent {
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
        let title = match action {
            1 => "Create worktree?",
            2 => "Remove worktree?",
            3 => "Merge worktree?",
            _ => "Stop worktree processes?",
        };
        self.run_in(
            "worktree",
            async move {
                let project = Some(
                    workspace
                        .ok_or("Project is unavailable.")?
                        .project
                        .ok_or("Project is unavailable.")?,
                );
                let plan = api::git_service_client::GitServiceClient::new(host.channel()?)
                    .preflight(api::PreflightRequest {
                        project: project.clone(),
                        intent: Some(api::preflight_request::Intent::Worktree(intent.clone())),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if !plan.can_execute {
                    return Err(plan.reason);
                }
                Ok(Update::Confirm(
                    title.into(),
                    plan.impacts
                        .iter()
                        .map(|impact| format!("• {impact}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    Box::pin(async move {
                        host.git_worktree(api::ChangeWorktreeRequest {
                            command: None,
                            project,
                            intent: Some(intent),
                            expected: plan.expected,
                        })
                        .await?;
                        Ok(Update::Done(
                            "Worktree change sent. Refresh to see its status.".into(),
                        ))
                    }),
                ))
            },
            cx,
        );
    }
    fn git_operation(&mut self, operation: u8, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        let message = self.value("commit", cx);
        let sign = self.git_sign;
        let upstream_branch = nonempty(self.value("upstream_branch", cx));
        let remote = nonempty(self.value("remote", cx)).unwrap_or("origin".into());
        let scope = match operation {
            0 => "commit",
            1 => "pull",
            _ => "push",
        };
        self.run_in(
            scope,
            async move {
                let project = Some(
                    workspace
                        .ok_or("Project is unavailable.")?
                        .project
                        .ok_or("Project is unavailable.")?,
                );
                let intent = if operation == 0 {
                    api::preflight_request::Intent::Commit(api::GitCommitIntent {
                        message: message.clone(),
                        sign,
                    })
                } else {
                    api::preflight_request::Intent::Network(api::GitNetworkIntent {
                        action: operation as i32,
                        remote: remote.clone(),
                        upstream_branch: upstream_branch.clone(),
                    })
                };
                let plan = api::git_service_client::GitServiceClient::new(host.channel()?)
                    .preflight(api::PreflightRequest {
                        project: project.clone(),
                        intent: Some(intent),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if !plan.can_execute {
                    return Err(plan.reason);
                }
                if operation == 0 {
                    host.git_commit(api::CommitRequest {
                        command: None,
                        project,
                        message,
                        expected: plan.expected,
                        sign,
                    })
                    .await?;
                } else if operation == 1 {
                    host.git_pull(api::PullRequest {
                        command: None,
                        project,
                        remote,
                        expected: plan.expected,
                        upstream_branch,
                    })
                    .await?;
                } else {
                    host.git_push(api::PushRequest {
                        command: None,
                        project,
                        remote,
                        expected: plan.expected,
                        upstream_branch,
                    })
                    .await?;
                }
                Ok(Update::Done(
                    match operation {
                        0 => "Commit sent. Refresh to see the result.",
                        1 => "Pull sent. Refresh to see the result.",
                        _ => "Push sent. Refresh to see the result.",
                    }
                    .into(),
                ))
            },
            cx,
        );
    }
}

pub(super) fn file_view_key(file: &p::FileRef) -> String {
    format!(
        "file:{}:{}",
        file.target
            .as_ref()
            .map(|target| format!("{target:?}"))
            .unwrap_or_default(),
        file.relative_path
    )
}
