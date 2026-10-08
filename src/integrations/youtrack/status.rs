//! Issue-scoped status choices and minimal updates, separate from form drafts.
use super::*;
use serde_json::json;

impl YoutrackField {
    pub fn is_status(&self) -> bool {
        matches!(
            self.kind,
            YoutrackFieldKind::State | YoutrackFieldKind::StateMachine
        )
    }

    pub fn status_label(&self) -> String {
        self.value["name"]
            .as_str()
            .or(self.value["presentation"].as_str())
            .unwrap_or("No status")
            .to_owned()
    }

    /// State machines expose actions, not an unrestricted list of state values.
    pub fn status_choices(&self) -> Vec<TaskOption> {
        match self.kind {
            YoutrackFieldKind::StateMachine => self.events.clone(),
            YoutrackFieldKind::State => self
                .allowed
                .iter()
                .filter(|value| value["archived"] != true)
                .filter_map(|value| {
                    Some(TaskOption {
                        value: value["id"].as_str()?.to_owned(),
                        label: value["name"].as_str()?.to_owned(),
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// None means the confirmed current state was selected again, not a write.
    pub fn status_update(&self, choice: &str) -> Result<Option<Value>, String> {
        if !self.is_status() || self.read_only || self.multi_value {
            return Err(format!(
                "{} cannot be changed with the status selector.",
                self.name
            ));
        }
        if self.kind == YoutrackFieldKind::State && self.value["id"].as_str() == Some(choice) {
            return Ok(None);
        }
        validate_identifier(&self.id, "status field ID")?;
        if !self
            .status_choices()
            .iter()
            .any(|option| option.value == choice)
        {
            return Err(
                "This status or workflow transition is no longer available. Refresh the task."
                    .into(),
            );
        }
        // The chosen provider id is a JSON value; it is not interpolated into a URL.
        if choice.is_empty() || choice.len() > 1024 || choice.chars().any(char::is_control) {
            return Err("YouTrack returned an invalid status choice.".into());
        }
        Ok(Some(if self.kind == YoutrackFieldKind::StateMachine {
            json!({"id": self.id, "$type": "StateMachineIssueCustomField", "event": {"id": choice, "$type": "Event"}})
        } else {
            json!({"id": self.id, "$type": self.field_type, "value": {"id": choice}})
        }))
    }
}

pub(super) async fn load(y: &Youtrack, task: &TaskRef) -> Result<Vec<YoutrackField>, String> {
    let task = y.task(task).await?;
    let details = task.youtrack.ok_or("YouTrack returned no issue details.")?;
    let mut fields: Vec<_> = details
        .custom_fields
        .iter()
        .filter(|field| field.is_status())
        .cloned()
        .collect();
    for field in &mut fields {
        load_choices(y, field).await?;
    }
    Ok(fields)
}

pub(super) async fn load_choices(y: &Youtrack, field: &mut YoutrackField) -> Result<(), String> {
    // Do not load unrelated project fields (or bundles for workflow actions).
    if field.kind == YoutrackFieldKind::State && !field.read_only && !field.multi_value {
        schema::load_bundle_values(y, field).await?;
    }
    Ok(())
}
