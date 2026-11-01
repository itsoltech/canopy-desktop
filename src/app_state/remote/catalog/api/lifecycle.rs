use super::*;
use canopy_desktop::{
    command_bus::PreparedCommand,
    host::{CommandInput, CommandReceipt},
};

fn workspace_resource(workspace: WorkspaceId) -> PlanResource {
    PlanResource::Project(workspace_ref(workspace))
}
fn terminal_resource(terminal: String) -> PlanResource {
    PlanResource::Terminal(terminal)
}
impl CatalogRpc {
    pub(super) async fn api_preflight_project(
        &self,
        request: Request<p::PreflightCloseProjectRequest>,
    ) -> Result<Response<p::ProjectClosePlan>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let project = required(request.get_ref().project.clone())?;
        let (workspace, root) = self
            .api_workspace(project.clone(), &grant, host::CHAT)
            .await?;
        let preflight = self
            .bridge
            .close_project_preflight(workspace)
            .await
            .map_err(bridge_status)?;
        let presence = plans::presence(&self.auth, &root, &self.state).await?;
        let mut risks: Vec<_> = self
            .bridge
            .summaries()
            .await
            .map_err(bridge_status)?
            .iter()
            .filter(|s| preflight.chats.contains(&s.chat))
            .map(|s| p::CloseRisk {
                title: s.title.clone().unwrap_or_else(|| "New chat".into()),
                working: s.working,
                queued: s.unsent as u32,
                observers: s.observers as u32,
            })
            .collect();
        let title = self
            .bridge
            .workspaces()
            .await
            .map_err(bridge_status)?
            .into_iter()
            .find(|w| w.workspace == workspace)
            .map(|w| w.name)
            .unwrap_or_default();
        risks.insert(
            0,
            p::CloseRisk {
                title,
                working: preflight.running || preflight.working > 0,
                queued: preflight.unsent as u32,
                observers: preflight.observers as u32,
            },
        );
        let reason = if preflight.dirty_editors {
            Some("needs_host_interaction: unsaved editor files on the host".to_owned())
        } else if plans::active_dirty(&presence) {
            Some("Another connected device has unsaved edits. Save or discard them before closing the project.".into())
        } else {
            None
        };
        let dirty = presence.iter().map(|d| d.wire(&project)).collect();
        let plan_id = self.plans.lock().map_err(|_| poisoned())?.insert(
            grant.id(),
            workspace_resource(workspace),
            false,
            PlanKind::Project {
                preflight,
                root: root.clone(),
                dirty: presence,
            },
        )?;
        self.auth.resource(grant.id(), host::CHAT, &root).await?;
        let result = p::ProjectClosePlan {
            project: Some(project),
            plan_id,
            risks,
            dirty,
            can_execute: reason.is_none(),
            reason: reason.unwrap_or_default(),
        };
        if result.encoded_len() > host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Project close risks exceed the control limit.",
            ));
        }
        Ok(Response::new(result))
    }
    pub(super) async fn api_preflight_terminal(
        &self,
        request: Request<p::PreflightCloseTerminalRequest>,
    ) -> Result<Response<p::TerminalClosePlan>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let project = required(request.get_ref().project.clone())?;
        let (workspace, root) = self
            .api_workspace(project.clone(), &grant, host::TERMINAL)
            .await?;
        let terminal = request.get_ref().terminal_id.clone();
        let info = self.terminal(&terminal).await?;
        if info.root().to_str() != Some(root.as_str())
            || info.workspace() != Some(workspace_ref(workspace).as_str())
        {
            return Err(Status::not_found("Terminal belongs to another checkout."));
        }
        let summary = info.summary(self.auth.host_id());
        let risks = vec![p::CloseRisk {
            title: summary.title,
            working: summary.running,
            queued: 0,
            observers: 0,
        }];
        let plan_id = self.plans.lock().map_err(|_| poisoned())?.insert(
            grant.id(),
            terminal_resource(terminal.clone()),
            false,
            PlanKind::Terminal {
                id: terminal.clone(),
                root: root.clone(),
            },
        )?;
        self.auth
            .resource(grant.id(), host::TERMINAL, &root)
            .await?;
        Ok(Response::new(p::TerminalClosePlan {
            project: Some(project),
            terminal_id: terminal,
            plan_id,
            risks,
            can_execute: true,
            reason: String::new(),
        }))
    }
    async fn prepare_lifecycle(
        &self,
        grant: &DeviceGrant,
        meta: p::CommandMeta,
        project: &p::ProjectRef,
        command: CatalogCommand,
    ) -> Result<PreparedCommand, Status> {
        let bus = CommandBus::new(self.auth.client().clone());
        let principal =
            Principal::authenticated(grant, meta.command_epoch).map_err(storage_status)?;
        let envelope = decode::envelope(
            meta,
            ResourceRef::Project(super::project(project)?),
            Command::Catalog(command),
        )?;
        if let Some(prepared) = bus
            .prepare_catalog(envelope.clone(), principal.clone(), None)
            .await
            .map_err(storage_status)?
        {
            return Ok(prepared);
        }
        let scope = envelope.command.required_scope();
        let (_, root) = self.api_workspace(project.clone(), grant, scope).await?;
        bus.prepare_catalog(envelope, principal, Some((root.clone(), root)))
            .await
            .map_err(storage_status)?
            .ok_or_else(missing)
    }
    async fn reject_lifecycle(
        &self,
        input: CommandInput,
        scope: u32,
        error: Status,
    ) -> Result<CommandReceipt, Status> {
        if !matches!(
            error.code(),
            tonic::Code::Aborted | tonic::Code::FailedPrecondition | tonic::Code::NotFound
        ) {
            return Err(error);
        }
        let accepted = self
            .auth
            .client()
            .accept_runtime_command(input, scope)
            .await
            .map_err(storage_status)?;
        if accepted.state() != CommandState::Accepted {
            return Ok(accepted);
        }
        self.auth
            .client()
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Accepted,
                CommandState::Rejected,
                domain_error("conditions_changed", error.message()),
            )
            .await
            .map_err(storage_status)
    }
    pub(super) async fn api_close_project(
        &self,
        request: Request<p::CloseProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.state.accepting()?;
        let request = request.into_inner();
        let project = required(request.project)?;
        let meta = required(request.command)?;
        let rpc = self.clone();
        let owned = self.operations.begin()?;
        let wire = project.clone();
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let prepared = rpc
                .prepare_lifecycle(
                    &grant,
                    meta,
                    &project,
                    CatalogCommand::CloseProject {
                        plan: request.plan_id.clone(),
                    },
                )
                .await?;
            if let Some(prior) = prepared
                .prior()
                .filter(|prior| prior.state() != CommandState::Accepted)
            {
                return Ok::<_, Status>(prior.clone());
            }
            let input = prepared.runtime_input().clone();
            let (workspace, _) = match rpc.api_workspace(project, &grant, host::CHAT).await {
                Ok(value) => value,
                Err(error) => return rpc.reject_lifecycle(input, host::CHAT, error).await,
            };
            let resource = workspace_resource(workspace);
            let plan = rpc.plans.lock().map_err(|_| poisoned())?.take(
                &request.plan_id,
                grant.id(),
                &resource,
                input.key().id(),
            );
            let (preflight, root, dirty) = match plan {
                Ok(PlanKind::Project {
                    preflight,
                    root,
                    dirty,
                }) => (preflight, root, dirty),
                Ok(_) => return Err(Status::invalid_argument("The plan is not for a project.")),
                Err(error) => return rpc.reject_lifecycle(input, host::CHAT, error).await,
            };
            if input.authority_root() != Some(root.as_str()) {
                return Err(Status::permission_denied("Project authority changed."));
            }
            rpc.auth.resource(grant.id(), host::CHAT, &root).await?;
            if plans::presence(&rpc.auth, &root, &rpc.state).await? != dirty
                || plans::active_dirty(&dirty)
            {
                return rpc
                    .reject_lifecycle(
                        input,
                        host::CHAT,
                        Status::aborted("Unsaved edits changed; confirm the close again."),
                    )
                    .await;
            }
            rpc.bridge
                .close_project(RemoteKey::admitted(input), preflight)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Reconcile the original typed command."))??;
        self.catalog_receipt(&receipt, p::command_receipt::Resource::Project(wire))
            .await
    }
    pub(super) async fn api_close_terminal(
        &self,
        request: Request<p::CloseTerminalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.state.accepting()?;
        let request = request.into_inner();
        let project = required(request.project)?;
        let meta = required(request.command)?;
        let rpc = self.clone();
        let owned = self.operations.begin()?;
        let wire = project.clone();
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let prepared = rpc
                .prepare_lifecycle(
                    &grant,
                    meta,
                    &project,
                    CatalogCommand::CloseTerminal {
                        terminal: request.terminal_id.clone(),
                        plan: request.plan_id.clone(),
                    },
                )
                .await?;
            if let Some(prior) = prepared
                .prior()
                .filter(|prior| prior.state() != CommandState::Accepted)
            {
                return Ok::<_, Status>(prior.clone());
            }
            let input = prepared.runtime_input().clone();
            let (workspace, root) = match rpc.api_workspace(project, &grant, host::TERMINAL).await {
                Ok(value) => value,
                Err(error) => return rpc.reject_lifecycle(input, host::TERMINAL, error).await,
            };
            let resource = terminal_resource(request.terminal_id.clone());
            let plan = rpc.plans.lock().map_err(|_| poisoned())?.take(
                &request.plan_id,
                grant.id(),
                &resource,
                input.key().id(),
            );
            match plan {
                Ok(PlanKind::Terminal { id, root: planned })
                    if id == request.terminal_id && planned == root => {}
                Ok(_) => {
                    return Err(Status::invalid_argument(
                        "The plan is not for this terminal.",
                    ));
                }
                Err(error) => return rpc.reject_lifecycle(input, host::TERMINAL, error).await,
            }
            let accepted = rpc
                .auth
                .client()
                .accept_runtime_command(input, host::TERMINAL)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                return Ok(accepted);
            }
            let current = rpc.terminals.terminal(&request.terminal_id).await?;
            if current.as_ref().is_none_or(|info| {
                info.root().to_str() != Some(root.as_str())
                    || info.workspace() != Some(workspace_ref(workspace).as_str())
            }) {
                return rpc
                    .auth
                    .client()
                    .transition_runtime_command(
                        accepted.key().clone(),
                        CommandState::Accepted,
                        CommandState::Rejected,
                        domain_error(
                            "conditions_changed",
                            "Terminal is no longer available in this checkout.",
                        ),
                    )
                    .await
                    .map_err(storage_status);
            }
            if rpc
                .auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .is_err()
            {
                return rpc
                    .auth
                    .client()
                    .get_runtime_command(accepted.key().clone(), host::TERMINAL)
                    .await
                    .map_err(storage_status)?
                    .ok_or_else(|| Status::unknown("Reconcile the terminal close."));
            }
            let (state, result) = match rpc.terminals.close(request.terminal_id).await {
                Ok(()) => (CommandState::Succeeded, None),
                Err(error) => (
                    CommandState::Failed,
                    domain_error("operation_failed", error.message()),
                ),
            };
            rpc.auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Attempting,
                    state,
                    result,
                )
                .await
                .map_err(storage_status)
        })
        .await
        .map_err(|_| Status::unknown("Reconcile the original typed command."))??;
        self.catalog_receipt(&receipt, p::command_receipt::Resource::Project(wire))
            .await
    }
}
