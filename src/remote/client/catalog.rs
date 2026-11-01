//! Reading the host catalog: a snapshot arrives as typed pages between one boundary
//! cursor (`CatalogSnapshotPart`), staged and installed only whole at its last page;
//! chat and terminal deltas follow on the watch stream. A gap, a foreign boundary or a
//! lost stream drops the staging and the installed catalog stays as it was.
use super::{HostClient, rpc_error};
use crate::api::proto as p;
use prost::Message;
use tonic::transport::Channel;

/// Staged pages of one snapshot at most (the host's projection budget).
pub const MAX_CATALOG_BYTES: usize = 32 * 1024 * 1024;

/// The pages of one snapshot received so far.
#[derive(Default)]
pub struct CatalogStaging {
    staged: Option<(p::Cursor, p::Catalog, usize)>,
}

impl CatalogStaging {
    pub fn is_staging(&self) -> bool {
        self.staged.is_some()
    }
    /// Drops the pages received so far (disconnect, gap, delta during a snapshot).
    pub fn discard(&mut self) {
        self.staged = None;
    }
    /// Adds one page: the whole catalog and its boundary once `part.last`. A new
    /// first page starts over (the host restarted the snapshot, e.g. after the
    /// device's authorization changed). Any error drops the staging.
    pub fn push(
        &mut self,
        host: &str,
        part: p::CatalogSnapshotPart,
    ) -> Result<Option<(p::Cursor, p::Catalog)>, String> {
        let result = self.accept(host, part);
        if result.is_err() {
            self.discard();
        }
        result
    }
    fn accept(
        &mut self,
        host: &str,
        part: p::CatalogSnapshotPart,
    ) -> Result<Option<(p::Cursor, p::Catalog)>, String> {
        let boundary = part.boundary.ok_or("Catalog page without boundary.")?;
        let page = part.page.ok_or("Empty catalog page.")?;
        if page.environment.as_ref().map(|e| e.host_id.as_str()) != Some(host) {
            return Err("Catalog identity mismatch.".into());
        }
        let bytes = page.encoded_len();
        if part.first {
            self.staged = Some((
                boundary.clone(),
                p::Catalog {
                    environment: Some(p::EnvironmentRef {
                        host_id: host.into(),
                    }),
                    ..Default::default()
                },
                0,
            ));
        }
        let Some((expected, staged, total)) = self.staged.as_mut() else {
            return Err("Catalog page before the first page.".into());
        };
        if *expected != boundary {
            return Err("Catalog snapshot boundary changed. Reconnect to reload.".into());
        }
        *total += bytes;
        if *total > MAX_CATALOG_BYTES {
            return Err("resource_exhausted: the host catalog is too large.".into());
        }
        merge(staged, page)?;
        if !part.last {
            return Ok(None);
        }
        let (boundary, catalog, _) = self.staged.take().expect("staged above");
        Ok(Some((boundary, catalog)))
    }
}

/// Appends `page` to `staged`: a project continued from the previous page (same ID,
/// same metadata) gets the rest of its checkouts; any other repeat is invalid.
fn merge(staged: &mut p::Catalog, page: p::Catalog) -> Result<(), String> {
    for project in page.projects {
        match staged.projects.last_mut() {
            Some(last) if last.path == project.path => {
                if last.name != project.name
                    || last.path != project.path
                    || last.origin != project.origin
                {
                    return Err("Conflicting catalog project page.".into());
                }
                last.checkouts.extend(project.checkouts);
            }
            _ => {
                if staged.projects.iter().any(|p| p.path == project.path) {
                    return Err("Repeated catalog project.".into());
                }
                staged.projects.push(project);
            }
        }
    }
    staged.threads.extend(page.threads);
    staged.terminals.extend(page.terminals);
    Ok(())
}

/// A catalog kept current from one `WatchSummaries` stream.
#[derive(Default)]
pub struct CatalogFeed {
    staging: CatalogStaging,
    cursor: Option<p::Cursor>,
}

impl CatalogFeed {
    /// Applies `event` to `catalog`: `true` when the installed catalog changed. An
    /// error means the stream must be read again from a snapshot; `catalog` is then
    /// unchanged by the failed event (staged pages are dropped).
    pub fn apply(
        &mut self,
        host: &str,
        catalog: &mut Option<std::sync::Arc<p::Catalog>>,
        event: p::SummaryEvent,
    ) -> Result<bool, String> {
        use p::summary_event::Change;
        let next = event.cursor.ok_or("Missing catalog cursor.")?;
        let change = event.change.ok_or("Empty catalog event.")?;
        match change {
            Change::SnapshotPart(part) => {
                let Some((boundary, whole)) = self.staging.push(host, part)? else {
                    return Ok(false);
                };
                self.cursor = Some(boundary);
                *catalog = Some(std::sync::Arc::new(whole));
                return Ok(true);
            }
            Change::Snapshot(value) => {
                if value.environment.as_ref().map(|e| e.host_id.as_str()) != Some(host) {
                    return Err("Catalog identity mismatch.".into());
                }
                self.staging.discard();
                self.cursor = Some(next);
                *catalog = Some(std::sync::Arc::new(value));
                return Ok(true);
            }
            _ => {}
        }
        if self.staging.is_staging() {
            self.staging.discard();
            return Err("Catalog changed during a snapshot. Reconnect to reload.".into());
        }
        if self.cursor.as_ref().is_none_or(|old| {
            old.runtime_epoch != next.runtime_epoch
                || old.stream_id != next.stream_id
                || old.sequence + 1 != next.sequence
        }) {
            return Err("Catalog requires resynchronization. Reconnect to reload.".into());
        }
        let current = std::sync::Arc::make_mut(catalog.as_mut().ok_or("Missing catalog.")?);
        match change {
            Change::Thread(value) => {
                if value.thread.as_ref().is_none_or(|c| c.host_id != host) {
                    return Err("Chat identity mismatch.".into());
                }
                current.threads.retain(|c| c.thread != value.thread);
                current.threads.push(*value);
            }
            Change::Terminal(value) => {
                if value.terminal.as_ref().is_none_or(|t| t.host_id != host) {
                    return Err("Terminal identity mismatch.".into());
                }
                current.terminals.retain(|t| t.terminal != value.terminal);
                current.terminals.push(value);
            }
            Change::Deleted(value) => match value.resource {
                Some(p::catalog_deleted::Resource::ThreadCatalogId(chat)) => {
                    current.threads.retain(|c| c.catalog_id != chat)
                }
                Some(p::catalog_deleted::Resource::Terminal(terminal)) => current
                    .terminals
                    .retain(|t| t.terminal.as_ref() != Some(&terminal)),
                _ => return Err("Workspace catalog changed. Reconnect to reload.".into()),
            },
            Change::Snapshot(_) | Change::SnapshotPart(_) => unreachable!("handled above"),
        }
        self.cursor = Some(next);
        Ok(true)
    }
}

/// The whole catalog of `host` read page by page (`ReadCatalog`); nothing partial is
/// returned.
pub async fn read_catalog(channel: Channel, host: &str) -> Result<p::Catalog, String> {
    let mut stream = crate::api::proto::catalog_service_client::CatalogServiceClient::new(channel)
        .read_catalog(crate::api::proto::ReadCatalogRequest {
            environment: Some(crate::api::proto::EnvironmentRef {
                host_id: host.into(),
            }),
        })
        .await
        .map_err(rpc_error)?
        .into_inner();
    let mut staging = CatalogStaging::default();
    while let Some(event) = stream.message().await.map_err(rpc_error)? {
        let Some(p::summary_event::Change::SnapshotPart(part)) = event.change else {
            return Err("Unexpected catalog event.".into());
        };
        if let Some((_, catalog)) = staging.push(host, part)? {
            return Ok(catalog);
        }
    }
    Err("The catalog stream ended before the whole catalog.".into())
}

impl HostClient {
    /// The host's whole catalog now, read from the connected host.
    pub async fn snapshot_catalog(&self) -> Result<p::Catalog, String> {
        read_catalog(self.channel()?, self.id.as_str()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(sequence: u64) -> p::Cursor {
        p::Cursor {
            runtime_epoch: "e".into(),
            stream_id: "catalog/1".into(),
            sequence,
        }
    }
    fn chat(id: &str) -> p::CatalogThread {
        p::CatalogThread {
            thread: Some(p::ThreadRef {
                host_id: "h".into(),
                thread_id: id.into(),
            }),
            catalog_id: id.into(),
            summary: Some(p::ThreadStatus {
                title: id.into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn part(boundary: u64, first: bool, last: bool, chats: &[&str]) -> p::SummaryEvent {
        p::SummaryEvent {
            cursor: Some(cursor(boundary)),
            change: Some(p::summary_event::Change::SnapshotPart(
                p::CatalogSnapshotPart {
                    boundary: Some(cursor(boundary)),
                    page: Some(p::Catalog {
                        environment: Some(p::EnvironmentRef {
                            host_id: "h".into(),
                        }),
                        threads: chats.iter().map(|id| chat(id)).collect(),
                        ..Default::default()
                    }),
                    first,
                    last,
                },
            )),
        }
    }

    #[test]
    fn a_paged_snapshot_replaces_the_catalog_only_whole() {
        let mut feed = CatalogFeed::default();
        let mut catalog = Some(std::sync::Arc::new(p::Catalog {
            environment: Some(p::EnvironmentRef {
                host_id: "h".into(),
            }),
            threads: vec![chat("old")],
            ..Default::default()
        }));
        assert!(
            !feed
                .apply("h", &mut catalog, part(1, true, false, &["a"]))
                .unwrap()
        );
        assert_eq!(catalog.as_ref().unwrap().threads[0].catalog_id, "old");
        // A delta during the snapshot drops the staging; the old catalog stays.
        let delta = |sequence| p::SummaryEvent {
            cursor: Some(cursor(sequence)),
            change: Some(p::summary_event::Change::Thread(Box::new(chat("x")))),
        };
        assert!(feed.apply("h", &mut catalog, delta(2)).is_err());
        assert_eq!(catalog.as_ref().unwrap().threads.len(), 1);
        // A page of another snapshot never completes the dropped one.
        assert!(
            feed.apply("h", &mut catalog, part(1, false, true, &["b"]))
                .is_err()
        );
        assert_eq!(catalog.as_ref().unwrap().threads[0].catalog_id, "old");
        // A restarted snapshot installs all rows of all its pages at its last page.
        let mut feed = CatalogFeed::default();
        feed.apply("h", &mut catalog, part(4, true, false, &["a"]))
            .unwrap();
        feed.apply("h", &mut catalog, part(4, false, false, &["b"]))
            .unwrap();
        assert!(
            feed.apply("h", &mut catalog, part(4, false, true, &["c"]))
                .unwrap()
        );
        let titles: Vec<_> = catalog
            .as_ref()
            .unwrap()
            .threads
            .iter()
            .map(|c| c.catalog_id.as_str())
            .collect();
        assert_eq!(titles, ["a", "b", "c"]);
        // Deltas continue after the boundary.
        let sidebar_snapshot = catalog.clone().unwrap();
        assert!(feed.apply("h", &mut catalog, delta(5)).unwrap());
        assert_eq!(
            sidebar_snapshot.threads.len(),
            3,
            "an installed sidebar snapshot is immutable"
        );
        assert_eq!(catalog.as_ref().unwrap().threads.len(), 4);
        let deleted = p::SummaryEvent {
            cursor: Some(cursor(6)),
            change: Some(p::summary_event::Change::Deleted(p::CatalogDeleted {
                resource: Some(p::catalog_deleted::Resource::ThreadCatalogId("x".into())),
            })),
        };
        assert!(feed.apply("h", &mut catalog, deleted).unwrap());
        assert_eq!(catalog.as_ref().unwrap().threads.len(), 3);
        assert!(
            catalog
                .as_ref()
                .unwrap()
                .threads
                .iter()
                .all(|c| c.catalog_id != "x")
        );
        assert!(feed.apply("h", &mut catalog, delta(8)).is_err());
    }
}
