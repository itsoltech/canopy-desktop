//! History pages and search over the whole saved conversation (SYNC-05, CHAT-12):
//! bounded keyset reads with stable keys; a page token is invalidated by a host
//! restart, another history epoch or another chat.
use super::projection::{Context, Frame, Out, Part, Slot, project};
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use canopy_desktop::{
    chat::transcript as t,
    remote::transcript as wire,
    settings::{RemoteHistoryPage, RemoteHistoryRequest},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageToken {
    epoch: String,
    chat: String,
    history_epoch: u64,
    kind: String,
    upper: Option<(u64, u64)>,
    before: Option<(u64, u64)>,
    query: Option<String>,
}
impl ChatsRpc {
    fn page_token(
        &self,
        record: &ChatRecord,
        value: Option<String>,
        kind: &str,
    ) -> Result<Option<PageToken>, Status> {
        let Some(value) = value else {
            return Ok(None);
        };
        if value.len() > 8192 {
            return Err(Status::invalid_argument("Invalid history page token."));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| Status::invalid_argument("Invalid history page token."))?;
        let token: PageToken = serde_json::from_slice(&bytes)
            .map_err(|_| Status::invalid_argument("Invalid history page token."))?;
        if token.epoch != self.replay.epoch()
            || token.chat != record.id()
            || token.history_epoch != record.history_epoch()
            || token.kind != kind
        {
            return Err(Status::aborted(
                "The history context changed; start a new read.",
            ));
        }
        Ok(Some(token))
    }
    fn encode_token(
        &self,
        record: &ChatRecord,
        kind: &str,
        page: &RemoteHistoryPage,
        query: Option<String>,
    ) -> Result<Option<String>, Status> {
        if page.next_before.is_none() {
            return Ok(None);
        }
        let token = PageToken {
            epoch: self.replay.epoch().into(),
            chat: record.id().into(),
            history_epoch: record.history_epoch(),
            kind: kind.into(),
            upper: page.upper,
            before: page.next_before,
            query,
        };
        Ok(Some(
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&token)
                    .map_err(|_| Status::internal("History token could not be encoded."))?,
            ),
        ))
    }
    pub(super) async fn history(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: proto::ReadHistoryPageRequest,
    ) -> Result<RpcStream<proto::HistoryPageEvent>, Status> {
        let permit = self.reads.admit(grant.id())?;
        let token = self.page_token(&record, request.page_token, "page")?;
        let anchor = request
            .before
            .or(request.around)
            .map(|k| (k.history_epoch, k.item_id));
        if token.is_some() && anchor.is_some() {
            return Err(Status::invalid_argument(
                "Use a page token or an initial anchor, not both.",
            ));
        }
        let page = self
            .auth
            .client()
            .read_remote_history_page(RemoteHistoryRequest {
                chat: record.id().into(),
                before: token.as_ref().and_then(|t| t.before).or(anchor),
                upper: token.as_ref().and_then(|t| t.upper),
                limit: if request.limit == 0 {
                    100
                } else {
                    request.limit.min(100)
                },
            })
            .await
            .map_err(storage_status)?;
        let next = self.encode_token(&record, "page", &page, None)?;
        let mut lost = page.rows.skipped > 0;
        let mut source = Vec::new();
        for (epoch, id, payload) in page.rows.items {
            match serde_json::from_str::<t::Item>(&payload) {
                Ok(item) if item.id.0 == id => source.push((epoch, item)),
                _ => lost = true,
            }
        }
        let files = source
            .iter()
            .flat_map(|(_, i)| super::projection::image_files(i))
            .collect();
        let images = self.images(record.id(), files, HashMap::new()).await?;
        let context = Context::of(&self.auth, &record);
        let chat = required(reference(&self.auth, &record))?;
        let read_id = uuid::Uuid::new_v4().to_string();
        let mut out = Out::default();
        out.events.push_back(Part::Event(proto::HistoryPageEvent {
            thread: Some(chat.clone()),
            part: Some(proto::history_page_event::Part::Start(
                proto::HistoryPageStart {
                    read_id: read_id.clone(),
                    previous_page_token: next,
                    incomplete: lost,
                },
            )),
        }));
        for (epoch, item) in source {
            let mut projected = project(&item, epoch, 0, 0, &context, &images)?;
            // A stored turn end is saved by definition; a trimmed row says so itself.
            if let Some(proto::transcript_item::Body::TurnEnd(end)) = &mut projected.body {
                end.saved = true;
            }
            out.events
                .push_back(Part::Slot(Slot::new(projected, std::sync::Arc::new(item))));
        }
        out.events.push_back(Part::Event(proto::HistoryPageEvent {
            thread: Some(chat.clone()),
            part: Some(proto::history_page_event::Part::EndReadId(read_id)),
        }));
        out.chat = Some(chat);
        out.permit = Some(permit);
        let page = Page {
            out,
            host: self.state.watch(),
            auth: self.auth.clone(),
            grant,
            record,
        };
        Ok(Box::pin(futures_lite::stream::unfold(
            Some(page),
            |page| async move {
                let mut page = page?;
                match page.next().await {
                    Ok(Some(event)) => Some((Ok(event), Some(page))),
                    Ok(None) => None,
                    Err(status) => Some((Err(status), None)),
                }
            },
        )))
    }
    pub(super) async fn search(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: proto::SearchHistoryRequest,
    ) -> Result<proto::HistoryMatches, Status> {
        if request.query.trim().is_empty() || request.query.len() > 1024 {
            return Err(Status::invalid_argument(
                "Enter a search of at most 1024 bytes.",
            ));
        }
        let token = self.page_token(&record, request.page_token, "search")?;
        if token
            .as_ref()
            .is_some_and(|t| t.query.as_deref() != Some(&request.query))
        {
            return Err(Status::invalid_argument(
                "The page token belongs to another search.",
            ));
        }
        let mut before = token.as_ref().and_then(|t| t.before);
        let mut upper = token.as_ref().and_then(|t| t.upper);
        let needle = request.query.to_lowercase();
        let mut found = Vec::new();
        let mut incomplete = false;
        let mut continuation = None;
        // At most 1000 rows / 10MiB per request, so search cannot monopolize the
        // single writer. Clients explicitly request another page to continue.
        for _ in 0..10 {
            self.auth
                .resource(grant.id(), host::READ, record.authority_root())
                .await?;
            let page = self
                .auth
                .client()
                .read_remote_history_page(RemoteHistoryRequest {
                    chat: record.id().into(),
                    before,
                    upper,
                    limit: 100,
                })
                .await
                .map_err(storage_status)?;
            upper = page.upper;
            before = page.next_before;
            incomplete |= page.rows.skipped > 0;
            continuation =
                self.encode_token(&record, "search", &page, Some(request.query.clone()))?;
            for (epoch, id, payload) in &page.rows.items {
                let Ok(item) = serde_json::from_str::<t::Item>(payload) else {
                    incomplete = true;
                    continue;
                };
                let text = match item.body {
                    t::Body::User(u) => u.text,
                    t::Body::Text(b) | t::Body::Thinking(b) => b.text,
                    t::Body::Tool(t) => t.output.map(|o| o.text).unwrap_or_default(),
                    t::Body::Notice(n) => n.text,
                    t::Body::Shell(s) => s.output,
                    _ => continue,
                };
                if text.to_lowercase().contains(&needle) {
                    found.push(proto::HistoryMatch {
                        item: Some(wire::key(*epoch, *id)),
                        excerpt: canopy_desktop::chat::presentation::single_line(&text, 240),
                    });
                }
            }
            if before.is_none() || found.len() >= 100 {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok(proto::HistoryMatches {
            matches: found,
            next_page_token: continuation,
            incomplete,
        })
    }
}

impl Frame for proto::HistoryPageEvent {
    fn items(chat: &proto::ThreadRef, items: Vec<proto::TranscriptItem>) -> Self {
        Self {
            thread: Some(chat.clone()),
            part: Some(proto::history_page_event::Part::Items(proto::ItemChunk {
                items,
            })),
        }
    }
    fn fragment(chat: &proto::ThreadRef, fragment: proto::ItemFragment) -> Self {
        Self {
            thread: Some(chat.clone()),
            part: Some(proto::history_page_event::Part::Fragment(fragment)),
        }
    }
}

/// One history page transfer; the device grant is checked again when the host's
/// authorization or readiness changes during it.
struct Page {
    out: Out<proto::HistoryPageEvent>,
    host: tokio::sync::watch::Receiver<canopy_desktop::api::proto::HostStatus>,
    auth: RpcAuth,
    grant: DeviceGrant,
    record: ChatRecord,
}
impl Page {
    async fn next(&mut self) -> Result<Option<proto::HistoryPageEvent>, Status> {
        if self.host.has_changed().unwrap_or(true) {
            let readiness = self.host.borrow_and_update().readiness;
            if readiness == canopy_desktop::api::proto::HostReadiness::Draining as i32
                || readiness == canopy_desktop::api::proto::HostReadiness::StorageFault as i32
            {
                return Err(Status::unavailable("The host is closing; reconnect later."));
            }
            self.auth
                .resource(self.grant.id(), host::READ, self.record.authority_root())
                .await?;
        }
        Ok(self.out.next())
    }
}
