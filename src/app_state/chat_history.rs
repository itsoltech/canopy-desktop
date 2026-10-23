//! Saved chat conversations: chats report changed items here, one writer batches them
//! into SQLite off the UI thread. Closing a chat deletes its history and attachments;
//! quitting only flushes.
use canopy_desktop::chat::history::{self, EpochMeta, Saved};
use canopy_desktop::chat::transcript::Item;
use canopy_desktop::settings::{ChatHistoryBatch, SettingsClient};
use gpui_kit::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

/// How long changes gather before a write (streaming replies change many times a second).
const DEBOUNCE: Duration = Duration::from_millis(500);
/// A finished turn is written sooner.
const URGENT: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Pending {
    epochs: BTreeMap<u64, EpochMeta>,
    items: BTreeMap<(u64, u64), Item>,
}
impl Pending {
    fn merge(&mut self, newer: Pending) {
        self.epochs.extend(newer.epochs);
        self.items.extend(newer.items);
    }
}

#[derive(Default)]
pub struct ChatHistory {
    client: Option<SettingsClient>,
    pending: HashMap<String, Pending>,
    /// Chats closed for good: nothing more is written for them.
    deleted: HashSet<String>,
    writer: Option<Task<()>>,
    urgent: bool,
    quitting: bool,
    pub error: Option<String>,
}

impl ChatHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self, client: SettingsClient, cx: &mut Context<Self>) {
        self.client = Some(client);
        if !self.pending.is_empty() {
            self.write_soon(cx);
        }
    }

    /// Saved history of `chat`; empty when nothing was saved yet.
    pub fn load(&self, chat: &str, cx: &mut Context<Self>) -> Task<Result<Saved, String>> {
        let client = self.client.clone();
        let chat = chat.to_owned();
        cx.background_spawn(async move {
            let client = client.ok_or("Chat history is unavailable")?;
            let rows = client
                .load_chat_history(chat)
                .await
                .map_err(|error| error.to_string())?;
            Ok(history::parse(rows))
        })
    }

    /// Copies `source`'s conversation up to `cut` into the new chat `target` (a fork,
    /// DEC-092): saved rows with what is still waiting to be written, and the attached
    /// images its messages name. Written at once, so the new chat finds it on opening.
    pub fn fork(
        &self,
        source: &str,
        target: &str,
        cut: canopy_desktop::chat::fork::Cut,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let client = self.client.clone();
        let (source, target) = (source.to_owned(), target.to_owned());
        // Not written yet: the newest rows win over the saved ones.
        let (epochs, items) = self
            .pending
            .get(&source)
            .map_or_else(Default::default, |p| (p.epochs.clone(), p.items.clone()));
        cx.background_spawn(async move {
            let client = client.ok_or("Chat history is unavailable")?;
            let rows = client
                .load_chat_history(source.clone())
                .await
                .map_err(|error| error.to_string())?;
            let mut saved = history::parse(rows);
            for (index, meta) in epochs {
                match saved.epochs.iter().position(|e| e.index == index) {
                    Some(at) => saved.epochs[at].meta = meta,
                    None => saved.epochs.push(history::SavedEpoch {
                        index,
                        meta,
                        items: vec![],
                    }),
                }
            }
            saved.epochs.sort_by_key(|epoch| epoch.index);
            for ((index, _), item) in items {
                let Some(at) = saved.epochs.iter().position(|e| e.index == index) else {
                    continue;
                };
                let epoch = &mut saved.epochs[at];
                match epoch.items.iter().position(|old| old.id == item.id) {
                    Some(at) => epoch.items[at] = item,
                    None => epoch.items.push(item),
                }
            }
            for epoch in &mut saved.epochs {
                epoch.items.sort_by_key(|item| item.id.0);
            }
            let parts = canopy_desktop::chat::fork::cut(saved.epochs, &cut)
                .ok_or("This part of the conversation can't be forked")?;
            if let Ok(data) = canopy_desktop::platform::directories::data_dir() {
                let from = history::attachments_dir(&data, &source);
                let to = history::attachments_dir(&data, &target);
                for file in canopy_desktop::chat::fork::images(&parts) {
                    // A missing file only loses its preview, as in the source chat.
                    if std::fs::create_dir_all(&to).is_ok() {
                        let _ = std::fs::copy(from.join(&file), to.join(&file));
                    }
                }
            }
            let batch = ChatHistoryBatch {
                chat: target,
                epochs: parts
                    .iter()
                    .map(|epoch| (epoch.index, history::meta_json(&epoch.meta)))
                    .collect(),
                items: parts
                    .iter()
                    .flat_map(|epoch| {
                        epoch.items.iter().filter_map(|item| {
                            Some((epoch.index, item.id.0, history::item_json(item)?))
                        })
                    })
                    .collect(),
            };
            client
                .save_chat_history(vec![batch])
                .await
                .map_err(|error| error.to_string())
        })
    }

    /// Remembers items (latest version wins) and how their session began; `urgent`
    /// writes sooner (a turn ended).
    pub fn record(
        &mut self,
        chat: &str,
        epoch: u64,
        meta: Option<EpochMeta>,
        items: impl IntoIterator<Item = Item>,
        urgent: bool,
        cx: &mut Context<Self>,
    ) {
        if self.deleted.contains(chat) {
            return;
        }
        let pending = self.pending.entry(chat.to_owned()).or_default();
        if let Some(meta) = meta {
            pending.epochs.insert(epoch, meta);
        }
        for item in items {
            pending.items.insert((epoch, item.id.0), item);
        }
        self.urgent |= urgent;
        self.write_soon(cx);
    }

    fn write_soon(&mut self, cx: &mut Context<Self>) {
        if self.writer.is_some() || self.quitting || self.client.is_none() {
            return;
        }
        self.writer = Some(cx.spawn(async move |this, cx| {
            loop {
                // Changes gather for a while; a turn that ends meanwhile cuts it short.
                let mut waited = Duration::ZERO;
                while waited < DEBOUNCE {
                    cx.background_executor().timer(URGENT).await;
                    waited += URGENT;
                    let urgent = this
                        .update(cx, |this, _| std::mem::take(&mut this.urgent))
                        .unwrap_or(true);
                    if urgent {
                        break;
                    }
                }
                let Ok(Some((client, taken))) = this.update(cx, |this, _| {
                    let client = this.client.clone()?;
                    Some((client, std::mem::take(&mut this.pending)))
                }) else {
                    return;
                };
                let result = save(&this, &client, &taken, cx).await;
                let again = this
                    .update(cx, |this, cx| {
                        match result {
                            Ok(()) => this.error = None,
                            Err(error) => {
                                // Keep what failed under anything newer; the next change retries.
                                for (chat, older) in taken {
                                    if this.deleted.contains(&chat) {
                                        continue;
                                    }
                                    let newer = this.pending.remove(&chat).unwrap_or_default();
                                    let mut merged = older;
                                    merged.merge(newer);
                                    this.pending.insert(chat, merged);
                                }
                                this.error = Some(format!("Could not save chat history: {error}"));
                            }
                        }
                        let again =
                            this.error.is_none() && !this.pending.is_empty() && !this.quitting;
                        if !again {
                            this.writer = None;
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

    /// Writes everything pending (quit). An error keeps it pending.
    pub fn flush(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.quitting = true;
        let previous = self.writer.take();
        cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let (client, taken) = this
                .update(cx, |this, _| {
                    (this.client.clone(), std::mem::take(&mut this.pending))
                })
                .map_err(|error| error.to_string())?;
            if taken.is_empty() {
                return Ok(());
            }
            let client = client.ok_or("Chat history is unavailable")?;
            let result = save(&this, &client, &taken, cx).await;
            if let Err(error) = &result {
                let _ = this.update(cx, |this, _| {
                    for (chat, older) in taken {
                        let newer = this.pending.remove(&chat).unwrap_or_default();
                        let mut merged = older;
                        merged.merge(newer);
                        this.pending.insert(chat, merged);
                    }
                    this.error = Some(format!("Could not save chat history: {error}"));
                });
            }
            result
        })
    }
    /// Quit was cancelled: writing goes on.
    pub fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        if !self.pending.is_empty() {
            self.write_soon(cx);
        }
    }

    /// Prevent late writers from resurrecting a conversation whose pane is gone.
    pub fn mark_deleted(&mut self, chat: &str) {
        self.deleted.insert(chat.to_owned());
        self.pending.remove(chat);
    }

    /// Completes deletion after processes have released their files. Errors are
    /// returned to the owning cleanup operation, which retains the retry state.
    pub fn delete(&mut self, chat: &str, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.mark_deleted(chat);
        let client = self.client.clone();
        let chat = chat.to_owned();
        cx.spawn(async move |_, cx| {
            let client = client
                .ok_or("Chat history is unavailable; retry cleanup when settings have loaded.")?;
            client
                .delete_chat_history(chat.clone())
                .await
                .map_err(|e| e.to_string())?;
            cx.background_spawn(async move { remove_attachments(&[chat]) })
                .await
        })
    }

    /// Deletes every saved chat not open anymore (after the layout was restored).
    pub fn retain(&mut self, keep: Vec<String>, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |_, cx| {
            let removed = match client.retain_chat_history(keep.clone()).await {
                Ok(removed) => removed,
                Err(error) => {
                    eprintln!("Chat history cleanup: {error}");
                    return;
                }
            };
            cx.background_spawn(async move {
                let _ = remove_attachments(&removed);
                remove_orphan_attachments(&keep);
            })
            .await;
        })
        .detach();
    }
}

/// One chat's pending rows, taken out for serializing: epochs, items, item epochs.
type Taken = (String, Vec<(u64, EpochMeta)>, Vec<Item>, Vec<u64>);

async fn save(
    this: &WeakEntity<ChatHistory>,
    client: &SettingsClient,
    taken: &HashMap<String, Pending>,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let batches: Vec<Taken> = taken
        .iter()
        .map(|(chat, pending)| {
            (
                chat.clone(),
                pending
                    .epochs
                    .iter()
                    .map(|(e, m)| (*e, m.clone()))
                    .collect(),
                pending.items.values().cloned().collect(),
                pending.items.keys().map(|(epoch, _)| *epoch).collect(),
            )
        })
        .collect();
    let batches = cx
        .background_spawn(async move {
            batches
                .into_iter()
                .map(|(chat, epochs, items, item_epochs)| ChatHistoryBatch {
                    chat,
                    epochs: epochs
                        .iter()
                        .map(|(epoch, meta)| (*epoch, history::meta_json(meta)))
                        .collect(),
                    items: items
                        .iter()
                        .zip(item_epochs)
                        .filter_map(|(item, epoch)| {
                            Some((epoch, item.id.0, history::item_json(item)?))
                        })
                        .collect(),
                })
                .collect::<Vec<_>>()
        })
        .await;
    // A chat deleted while its rows were prepared isn't written back (its delete is
    // queued already, so a write queued now would bring it back).
    let deleted = this
        .read_with(cx, |this, _| this.deleted.clone())
        .unwrap_or_default();
    let batches = batches
        .into_iter()
        .filter(|batch| !deleted.contains(&batch.chat))
        .collect();
    client
        .save_chat_history(batches)
        .await
        .map_err(|error| error.to_string())
}

fn attachments_root() -> Option<std::path::PathBuf> {
    let data = canopy_desktop::platform::directories::data_dir().ok()?;
    Some(data.join("chat-attachments"))
}

/// Removes the attachment folders of `chats` (only UUID-named folders under the root).
fn remove_attachments(chats: &[String]) -> Result<(), String> {
    let Some(root) = attachments_root() else {
        return Err("Chat attachment storage is unavailable.".into());
    };
    for chat in chats {
        if uuid::Uuid::parse_str(chat).is_err() {
            continue;
        }
        let dir = root.join(chat);
        match std::fs::symlink_metadata(&dir) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| format!("Could not remove chat attachments: {e}"))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect chat attachments: {error}")),
            Ok(_) => {
                return Err("Chat attachments are not a plain directory; they were kept.".into());
            }
        }
    }
    Ok(())
}

/// Removes attachment folders of chats that no longer exist (e.g. history deleted
/// while Canopy was not running).
fn remove_orphan_attachments(keep: &[String]) {
    let Some(root) = attachments_root() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let orphans: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !keep.contains(name))
        .collect();
    let _ = remove_attachments(&orphans);
}
