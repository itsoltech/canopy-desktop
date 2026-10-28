use super::*;
use canopy_desktop::remote::client::rpc_error;

impl RemoteWindow {
    pub(super) fn close_project(&mut self, workspace: p::WorkspaceRef, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.run_in(
            "close-project",
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
fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}
/// Consequence first, then one line per affected chat or runtime.
fn close_risks(plan: &p::ClosePlan) -> String {
    let mut lines = vec!["Closing removes this project’s chats, stops its processes and disconnects its viewers. Drafts on this device stay available.".to_owned()];
    for risk in &plan.risks {
        lines.push(format!(
            "• {} — {}, {}, {}",
            risk.title,
            if risk.working { "running" } else { "idle" },
            plural(risk.queued, "queued message", "queued messages"),
            plural(risk.observers, "viewer", "viewers")
        ));
        for dirty in &risk.dirty {
            lines.push(format!(
                "• Unsaved {} on device {} — {}",
                dirty
                    .file
                    .as_ref()
                    .map(|f| f.relative_path.as_str())
                    .unwrap_or("file"),
                dirty.device_id,
                if dirty.online {
                    "its owner must save or discard it first"
                } else {
                    "the device is offline, so its draft may be lost"
                }
            ));
        }
    }
    if !plan.reason.is_empty() {
        lines.push(plan.reason.clone());
    }
    lines.join("\n")
}
