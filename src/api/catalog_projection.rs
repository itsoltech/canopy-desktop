//! The desktop retains its current projection until the resource APIs move in PR3.
use super::{projection::value, proto as p};
use crate::remote::proto as old;
use tonic::Status;

pub fn terminal(v: old::TerminalSummary) -> p::TerminalSummary {
    p::TerminalSummary {
        terminal: v.terminal.map(value),
        project_catalog_id: v.workspace.map(|w| w.workspace_id).unwrap_or_default(),
        title: v.title,
        running: v.running,
        input_device_id: v.input_device_id,
    }
}
pub fn legacy_thread(v: p::CatalogThread) -> Result<old::ChatSummary, Status> {
    let thread = v
        .thread
        .ok_or_else(|| Status::data_loss("Catalog thread identity is missing."))?;
    let summary = v
        .summary
        .ok_or_else(|| Status::data_loss("Catalog thread status is missing."))?;
    if summary.thread.as_ref() != Some(&thread) {
        return Err(Status::data_loss("Catalog thread identity differs."));
    }
    Ok(old::ChatSummary {
        chat: Some(old::ChatRef {
            host_id: thread.host_id.clone(),
            chat_id: v.catalog_id,
        }),
        workspace: v.project_catalog_id.map(|workspace_id| old::WorkspaceRef {
            host_id: thread.host_id,
            workspace_id,
        }),
        title: summary.title,
        agent_id: summary.agent_id,
        profile_id: summary.profile_id,
        state: summary.state,
        pending_decisions: summary.pending_decisions,
        queued_messages: summary.queued_messages,
        run_id: summary.run_id.map(|id| id.to_string()),
        history_epoch: summary.history_epoch,
        goal: summary.goal.map(value),
    })
}
pub fn legacy_terminal(v: p::TerminalSummary) -> old::TerminalSummary {
    let host_id = v
        .terminal
        .as_ref()
        .map(|t| t.host_id.clone())
        .unwrap_or_default();
    old::TerminalSummary {
        terminal: v.terminal.map(value),
        workspace: Some(old::WorkspaceRef {
            host_id,
            workspace_id: v.project_catalog_id,
        }),
        title: v.title,
        running: v.running,
        input_device_id: v.input_device_id,
    }
}
pub fn legacy_catalog(v: p::Catalog) -> Result<old::Catalog, Status> {
    let host_id = v
        .environment
        .ok_or_else(|| Status::data_loss("Catalog environment is missing."))?
        .host_id;
    let projects = v
        .projects
        .into_iter()
        .map(|v| old::ProjectSummary {
            project_id: v.path.clone(),
            path: v.path,
            name: v.name,
            origin: v.origin,
            checkouts: v
                .checkouts
                .into_iter()
                .map(|w| old::WorkspaceSummary {
                    workspace: Some(old::WorkspaceRef {
                        host_id: host_id.clone(),
                        workspace_id: w.catalog_id,
                    }),
                    name: w.name,
                    path: w.path,
                    closing: Some(w.closing),
                    branch: w.branch,
                    repository_path: w.repository_path,
                })
                .collect(),
        })
        .collect();
    Ok(old::Catalog {
        host_id,
        projects,
        chats: v
            .threads
            .into_iter()
            .map(legacy_thread)
            .collect::<Result<_, _>>()?,
        terminals: v.terminals.into_iter().map(legacy_terminal).collect(),
    })
}
pub fn legacy_event(v: p::SummaryEvent, host: &str) -> Result<old::SummaryEvent, Status> {
    use old::summary_event::Change as O;
    use p::summary_event::Change as N;
    let change = match v
        .change
        .ok_or_else(|| Status::data_loss("Catalog event is missing."))?
    {
        N::Snapshot(v) => O::Snapshot(legacy_catalog(v)?),
        N::Thread(v) => O::Chat(legacy_thread(*v)?),
        N::Terminal(v) => O::Terminal(legacy_terminal(v)),
        N::Deleted(v) => O::Deleted(old::ResourceRef {
            resource: Some(
                match v
                    .resource
                    .ok_or_else(|| Status::data_loss("Deleted resource is missing."))?
                {
                    p::catalog_deleted::Resource::ThreadCatalogId(chat_id) => {
                        old::resource_ref::Resource::Chat(old::ChatRef {
                            host_id: host.into(),
                            chat_id,
                        })
                    }
                    p::catalog_deleted::Resource::Terminal(v) => {
                        old::resource_ref::Resource::Terminal(value(v))
                    }
                },
            ),
        }),
        N::SnapshotPart(v) => O::SnapshotPart(old::CatalogSnapshotPart {
            boundary: v.boundary.map(value),
            page: v.page.map(legacy_catalog).transpose()?,
            first: v.first,
            last: v.last,
        }),
    };
    Ok(old::SummaryEvent {
        cursor: v.cursor.map(value),
        change: Some(change),
    })
}
