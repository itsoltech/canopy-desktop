//! Read-only scrollback pages. The host's own display offset and selection never
//! change. A page token binds run, captured projection revision and offset; any
//! output in between makes it stale (a fresh read, never a mixed page).
use super::{TerminalInfo, TerminalRpc, check_run, projection, proto};
use sha2::{Digest, Sha256};
use tonic::Status;

/// Rows a client may read at once.
pub(super) const PAGE_ROWS: u32 = 200;
type Material = Option<crate::remote::sync::ByteReservation>;
pub(super) type Events = Box<dyn Iterator<Item = (proto::TerminalHistoryEvent, Material)> + Send>;

fn mac(rpc: &TerminalRpc, run: &str, revision: u64, offset: u64) -> String {
    let digest = Sha256::new()
        .chain_update(rpc.token_key.as_slice())
        .chain_update(format!("{run}\n{revision}\n{offset}").as_bytes())
        .finalize();
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn token(rpc: &TerminalRpc, run: &str, revision: u64, offset: u64) -> String {
    format!("{offset}.{revision}.{}", mac(rpc, run, revision, offset))
}
/// The offset a token names, if it was issued for this run at `revision`.
pub(super) fn resolve(
    rpc: &TerminalRpc,
    run: &str,
    revision: u64,
    token: &str,
) -> Result<u64, Status> {
    let invalid = || Status::invalid_argument("Invalid terminal history token.");
    let mut parts = token.splitn(3, '.');
    let offset: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(invalid)?;
    let issued: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(invalid)?;
    if parts.next() != Some(mac(rpc, run, issued, offset).as_str()) {
        return Err(invalid());
    }
    if issued != revision {
        return Err(Status::aborted(
            "The terminal changed since this page. Read the history again.",
        ));
    }
    Ok(offset)
}

pub(super) async fn read(
    rpc: &TerminalRpc,
    info: &TerminalInfo,
    request: proto::TerminalHistoryRequest,
) -> Result<Events, Status> {
    if request.rows == 0 || request.rows > PAGE_ROWS {
        return Err(Status::invalid_argument(
            "Terminal history pages have 1 to 200 rows.",
        ));
    }
    let run = check_run(info, &request.run_id)?;
    let session = run.session().clone();
    // The parser holds the grid only briefly; wait for it instead of failing.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let (page, reservation) = loop {
        let revision = session.revision();
        let offset = match &request.page_token {
            Some(token) => resolve(rpc, run.id(), revision, token)?,
            None => u64::from(request.offset_from_bottom),
        };
        match session.history_page_bounded(
            offset as usize,
            request.rows as usize,
            |cells, text| projection::admit(&rpc.output, cells, text),
        )? {
            // Without a token any consistent page is valid; a token's offset is
            // valid only for the revision it was issued at.
            Some((page, reservation))
                if request.page_token.is_none() || page.frame.revision == revision =>
            {
                break (page, reservation);
            }
            Some(_) => {
                return Err(Status::aborted(
                    "The terminal changed since this page. Read the history again.",
                ));
            }
            None => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(Status::deadline_exceeded(
                        "The terminal is busy. Read the history again.",
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
    };
    let revision = page.frame.revision;
    let boundary = proto::Cursor {
        runtime_epoch: rpc.epoch.to_string(),
        stream_id: format!("terminal-history/{}", info.id()),
        sequence: revision,
    };
    let event = |part| proto::TerminalHistoryEvent {
        terminal: Some(rpc.reference(info.id())),
        run_id: run.id().into(),
        part: Some(part),
    };
    use proto::terminal_history_event::Part;
    let start = event(Part::Start(proto::TerminalHistoryStart {
        boundary: Some(boundary.clone()),
        offset_from_bottom: page.frame.offset as u32,
        retained_rows: page.retained as u32,
        lost_scrollback: page.lost,
        previous_page_token: page
            .previous
            .map(|offset| token(rpc, run.id(), revision, offset as u64)),
    }));
    let terminal = rpc.reference(info.id());
    let run_id = run.id().to_owned();
    let frames: Box<dyn Iterator<Item = (Part, Material)> + Send> =
        match projection::page_bounded(page.frame, page.lost, reservation)? {
            projection::Encoded::Inline(frame, reservation) => {
                Box::new(std::iter::once((Part::Frame(frame), reservation)))
            }
            projection::Encoded::Fragments(fragments) => {
                Box::new(fragments.map(|fragment| (Part::FrameFragment(fragment), None)))
            }
        };
    let end = event(Part::End(boundary));
    Ok(Box::new(
        std::iter::once((start, None))
            .chain(frames.map(move |(part, material)| {
                (
                    proto::TerminalHistoryEvent {
                        terminal: Some(terminal.clone()),
                        run_id: run_id.clone(),
                        part: Some(part),
                    },
                    material,
                )
            }))
            .chain(std::iter::once((end, None))),
    ))
}

/// The producer owns the page workspace and releases it on idle timeout, even
/// when the transport stops polling. Only one event is queued ahead of polling.
pub(super) fn stream(
    rpc: TerminalRpc,
    owner: String,
    root: String,
    mut events: Events,
    slot: super::StreamSlot,
) -> super::RpcStream<proto::TerminalHistoryEvent> {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (finished, completion) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _slot = slot;
        let outcome = async {
            loop {
                let deadline = tokio::time::sleep(std::time::Duration::from_secs(5));
                tokio::pin!(deadline);
                let mut recheck = tokio::time::interval(std::time::Duration::from_millis(500));
                let permit = loop {
                    tokio::select! {
                        permit = sender.reserve() => break permit.map_err(|_| Status::cancelled("Terminal history reader disconnected."))?,
                        _ = &mut deadline => return Err(Status::resource_exhausted("Terminal history is not being read. Read the page again.")),
                        _ = recheck.tick() => {
                            if let Some(state) = &rpc.state { state.accepting()?; }
                            rpc.auth.resource(&owner, crate::host::READ, &root).await?;
                        }
                    }
                };
                rpc.auth.resource(&owner, crate::host::READ, &root).await?;
                let Some((event, material)) = events.next() else { return Ok::<_, Status>(()); };
                let charge = rpc.output.reserve_output(&owner, prost::Message::encoded_len(&event))
                    .map_err(|_| Status::resource_exhausted("Terminal output budget is full. Read the page again."))?;
                permit.send((event, charge, material));
            }
        }.await;
        drop(events);
        let _ = finished.send(outcome);
    });
    Box::pin(futures_lite::stream::unfold(
        (receiver, Some(completion)),
        |(mut receiver, mut completion)| async move {
            if let Some(mut finished) = completion.take() {
                tokio::select! {
                    biased;
                    result = &mut finished => {
                        if let Err(error) = result.unwrap_or_else(|_| Err(Status::unavailable("Terminal history ended. Read the page again."))) {
                            receiver.close();
                            while receiver.try_recv().is_ok() {}
                            return Some((Err(error), (receiver, None)));
                        }
                        // Success drains the queued End before EOF.
                    }
                    event = receiver.recv() => {
                        if let Some((event, _charge, _material)) = event {
                            return Some((Ok(event), (receiver, Some(finished))));
                        }
                        if let Err(error) = finished.await.unwrap_or_else(|_| Err(Status::unavailable("Terminal history ended. Read the page again."))) {
                            return Some((Err(error), (receiver, None)));
                        }
                        return None;
                    }
                }
            }
            receiver
                .recv()
                .await
                .map(|(event, _charge, _material)| (Ok(event), (receiver, None)))
        },
    ))
}
