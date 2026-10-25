//! Notes match the local scratchpad: device memory, scoped to host and checkout.
use super::*;
impl RemoteWindow {
    fn note_key(&self) -> Option<String> {
        Some(format!(
            "{}:{}",
            self.host.as_ref()?.id().as_str(),
            self.workspace.as_ref()?.workspace_id
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
                    let mut state = TextareaState::new(window, cx);
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
    pub(super) fn render_notes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(input) = self
            .note_key()
            .and_then(|key| self.notes.get(&key))
            .cloned()
        else {
            return div()
                .child("Select a workspace in Projects, then open Notes.")
                .into_any_element();
        };
        column().gap_2().child("Notes").child("Scratchpad for this workspace on this device. Notes last until Canopy closes; export to keep them.")
            .child(textarea(&input).h_96())
            .child(button("preview-notes","Preview Markdown…").on_click(cx.listener(|this,_,_,cx| {
                if let Some(input)=this.note_key().and_then(|key|this.notes.get(&key)) {
                    this.apply(Update::TextPreview("Notes".into(),input.read(cx).value().to_string(),true),cx);
                }
            })))
            .child(self.field("path","Export to workspace-relative file"))
            .child(button("export-notes","Export notes…").disabled(!self.ready()).on_click(cx.listener(|this,_,_,cx| {
                let Some(input)=this.note_key().and_then(|key|this.notes.get(&key)) else {return;};
                let bytes=input.read(cx).value().as_bytes().to_vec();
                let host=this.host.clone().unwrap();
                let file=p::FileRef{workspace:this.workspace.clone(),relative_path:this.value("path",cx)};
                let detail=format!("Create {} on the host. Existing files will not be overwritten.",file.relative_path);
                this.apply(Update::Confirm("Export notes?".into(),detail,Box::pin(async move {
                    let artifact=host.upload(p::ResourceRef{resource:Some(p::resource_ref::Resource::File(file.clone()))},"file-save","text/markdown",bytes).await?;
                    host.mutate(None,move|channel,command|async move {p::files_service_client::FilesServiceClient::new(channel).create_file(p::CreateFileRequest{command:Some(command),file:Some(file),directory:false,content:Some(artifact)}).await}).await?;
                    Ok(Update::Done("Notes exported. The scratchpad is still available.".into()))
                })),cx);
            }))).into_any_element()
    }
}
