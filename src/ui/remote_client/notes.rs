//! Notes match the local scratchpad: device memory, scoped to host and checkout.
use super::*;
use chrome::*;
use gpui_kit::component::{IconName, button::ButtonVariants};

impl RemoteWindow {
    fn note_key(&self) -> Option<String> {
        Some(format!(
            "{}:{}",
            self.host.as_ref()?.id().as_str(),
            self.workspace.as_ref()?.catalog_id
        ))
    }
    pub(super) fn open_notes(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.note_key() else {
            return;
        };
        if !self.notes.contains_key(&key) {
            let text = self
                .global
                .notes
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let state = self.window.update(cx, |_, window, cx| {
                cx.new(|cx| {
                    let mut state =
                        TextareaState::new(window, cx).placeholder("Write notes in Markdown…");
                    state.set_value(text, window, cx);
                    state
                })
            });
            if let Ok(state) = state {
                let note_key = key.clone();
                let notes = self.global.notes.clone();
                self.subscriptions.push(cx.subscribe(
                    &state,
                    move |_, state, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            notes
                                .lock()
                                .unwrap()
                                .insert(note_key.clone(), state.read(cx).value().to_string());
                        }
                    },
                ));
                self.notes.insert(key, state);
            }
        }
        self.navigate(Page::Notes, cx);
    }
    /// Names a new file in the checkout; the host refuses to replace an existing one.
    fn export_notes(&mut self, cx: &mut Context<Self>) {
        let folder = self.value("path", cx);
        let suggestion = if folder.trim_matches('/').is_empty() {
            "notes.md".to_owned()
        } else {
            format!("{}/notes.md", folder.trim_matches('/'))
        };
        self.open_prompt(
            "Export notes",
            "New file in this checkout. Existing files are never replaced.".into(),
            suggestion,
            "Export",
            |this, path, cx| {
                if path.is_empty() {
                    return Err("Enter a file path.".into());
                }
                let Some(input) = this.note_key().and_then(|key| this.notes.get(&key)) else {
                    return Err("Select a checkout first.".into());
                };
                let Some(host) = this.host.clone() else {
                    return Err("Select a computer first.".into());
                };
                if !this.ready() {
                    return Err("Connect to the computer first.".into());
                }
                let bytes = input.read(cx).value().as_bytes().to_vec();
                let file = p::FileRef {
                    target: this.workspace.as_ref().and_then(project_target),
                    relative_path: path.clone(),
                };
                this.run_in(
                    "notes",
                    async move {
                        let artifact = host
                            .upload(
                                canopy_desktop::remote::client::UploadTarget::File(file.clone()),
                                "file_save",
                                "text/markdown",
                                bytes,
                            )
                            .await?;
                        host.create_file(file, false, Some(artifact)).await?;
                        Ok(Update::Done(format!(
                            "Exported to {path}. The notes stay here as well."
                        )))
                    },
                    cx,
                );
                Ok(())
            },
            cx,
        );
    }
    pub(super) fn render_notes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(input) = self
            .note_key()
            .and_then(|key| self.notes.get(&key))
            .cloned()
        else {
            return empty(
                "No checkout selected",
                "Notes belong to a checkout. Choose one in Projects to take notes.",
            )
            .children((!self.embedded && self.host.is_some()).then(|| {
                button("open-projects", "Open Projects")
                    .on_click(cx.listener(|this, _, _, cx| this.show_page(Page::Catalog, cx)))
            }))
            .into_any_element();
        };
        column()
            .flex_1()
            .min_h_0()
            .gap(px(8.))
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("Only on this device. Kept until Canopy quits; export to keep a copy."),
                    )
                    .child(
                        button("preview-notes", "Preview…")
                            .ghost()
                            .border_0()
                            .icon(icon(IconName::Eye))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(input) =
                                    this.note_key().and_then(|key| this.notes.get(&key))
                                {
                                    let text = input.read(cx).value().to_string();
                                    this.apply(Update::TextPreview("Notes".into(), text, true), cx);
                                }
                            })),
                    )
                    .child(
                        button("export-notes", "Export…")
                            .loading(self.pending == Some("notes"))
                            .disabled(!self.ready())
                            .on_click(cx.listener(|this, _, _, cx| this.export_notes(cx))),
                    ),
            )
            .children(self.notice("notes"))
            .child(textarea(&input).flex_1().min_h(px(200.)).text_size(px(13.)))
            .into_any_element()
    }
}
