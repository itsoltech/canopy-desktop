use super::*;
use canopy_desktop::remote::client::rpc_error;

impl RemoteWindow {
    pub(super) fn close_project(&mut self, workspace: p::WorkspaceRef, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.run(
            async move {
                let resource = Some(p::ResourceRef {
                    resource: Some(p::resource_ref::Resource::Workspace(workspace)),
                });
                let plan = p::catalog_service_client::CatalogServiceClient::new(host.channel()?)
                    .preflight_close(p::PreflightCloseRequest {
                        resource: resource.clone(),
                        delete: false,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                let detail = close_risks(&plan);
                if !plan.can_execute {
                    return Err(detail);
                }
                Ok(Update::Confirm(
                    "Close project on host?".into(),
                    detail,
                    Box::pin(async move {
                        let receipt = host
                            .mutate(None, move |channel, command| async move {
                                p::catalog_service_client::CatalogServiceClient::new(channel)
                                    .close_resource(p::CloseResourceRequest {
                                        command: Some(command),
                                        resource,
                                        plan_id: plan.plan_id,
                                    })
                                    .await
                            })
                            .await?;
                        Ok(Update::Done(
                            if receipt.state == p::OperationState::Succeeded as i32 {
                                "Project closed on host. Local drafts remain available."
                            } else {
                                "Close accepted. Review the operation result before retrying."
                            }
                            .into(),
                        ))
                    }),
                ))
            },
            cx,
        );
    }
}
fn close_risks(plan: &p::ClosePlan) -> String {
    let mut lines = vec!["Closing removes this project’s chats, stops its runtimes and detaches its viewers. Local device drafts remain available.".to_owned()];
    for risk in &plan.risks {
        lines.push(format!(
            "{}: {}, {} queued messages, {} viewers",
            risk.title,
            if risk.working { "running" } else { "idle" },
            risk.queued,
            risk.observers
        ));
        for dirty in &risk.dirty {
            lines.push(format!(
                "Unsaved {} · device {} · {}",
                dirty
                    .file
                    .as_ref()
                    .map(|f| f.relative_path.as_str())
                    .unwrap_or("file"),
                dirty.device_id,
                if dirty.online {
                    "online — owner must save or discard"
                } else {
                    "offline — draft remains at risk"
                }
            ));
        }
    }
    if !plan.reason.is_empty() {
        lines.push(plan.reason.clone());
    }
    lines.join("\n")
}
