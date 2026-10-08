use canopy_desktop::{
    integrations::drafts::{TaskDraft, TaskDrafts},
    settings::SettingsClient,
};
use gpui_kit::*;
use std::time::Duration;
pub struct DraftsState {
    pub ready: bool,
    pub error: Option<String>,
    pub values: TaskDrafts,
    client: Option<SettingsClient>,
    writer: Option<Task<()>>,
    revision: u64,
    saved: u64,
    quitting: bool,
}
impl DraftsState {
    pub fn new() -> Self {
        Self {
            ready: false,
            error: None,
            values: Default::default(),
            client: None,
            writer: None,
            revision: 0,
            saved: 0,
            quitting: false,
        }
    }
    pub fn initialize(&mut self, client: SettingsClient, cx: &mut Context<Self>) {
        self.client = Some(client.clone());
        self.writer = Some(cx.spawn(async move |this, cx| {
            let result = client.load_task_drafts().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(values) => {
                        this.values = values;
                        this.ready = true;
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                this.writer = None;
                cx.notify();
            });
        }));
    }
    pub fn set(&mut self, key: String, draft: TaskDraft, cx: &mut Context<Self>) {
        if !self.ready || self.quitting || self.values.get(&key) == Some(&draft) {
            return;
        }
        let mut draft = draft;
        if let Some(old) = self.values.get(&key)
            && old.value == draft.value
            && old.baseline == draft.baseline
        {
            draft.warning = old.warning.clone();
        }
        if self.values.get(&key) == Some(&draft) {
            return;
        }
        self.values.insert(key, draft);
        self.revision += 1;
        self.persist(cx);
        cx.notify();
    }
    pub fn record_error(&mut self, key: &str, error: String, cx: &mut Context<Self>) {
        if let Some(draft) = self.values.get_mut(key) {
            draft.warning = Some(error);
            self.revision += 1;
            self.persist(cx);
            cx.notify();
        }
    }
    pub fn remove(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.values.remove(key).is_none() {
            return;
        }
        self.revision += 1;
        self.persist(cx);
        cx.notify();
    }
    fn persist(&mut self, cx: &mut Context<Self>) {
        if self.writer.is_some() || self.quitting {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.writer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let Ok((values, revision)) =
                    this.read_with(cx, |s, _| (s.values.clone(), s.revision))
                else {
                    return;
                };
                let result = client.save_task_drafts(values).await;
                let again = this
                    .update(cx, |s, cx| {
                        match result {
                            Ok(()) => {
                                s.saved = revision;
                                s.error = None;
                            }
                            Err(error) => {
                                s.error = Some(format!("Could not save task drafts: {error}"))
                            }
                        }
                        let again = s.error.is_none() && s.revision != s.saved && !s.quitting;
                        if !again {
                            s.writer = None;
                        }
                        cx.notify();
                        again
                    })
                    .unwrap_or(false);
                if !again {
                    break;
                }
            }
        }));
    }
    pub fn pause(&mut self) {
        self.quitting = true;
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        if self.revision != self.saved {
            self.persist(cx);
        }
        cx.notify();
    }
    pub fn flush(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        let previous = self.writer.take();
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let (ready, values, revision, saved) = this
                .read_with(cx, |s, _| (s.ready, s.values.clone(), s.revision, s.saved))
                .map_err(|e| e.to_string())?;
            if !ready || revision == saved {
                return Ok(());
            } // A failed load has never accepted edits.
            let client = client.ok_or("Draft storage is unavailable")?;
            client
                .save_task_drafts(values)
                .await
                .map_err(|e| e.to_string())?;
            let _ = this.update(cx, |s, cx| {
                s.saved = revision;
                s.error = None;
                cx.notify();
            });
            Ok(())
        })
    }
}
