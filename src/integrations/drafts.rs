use super::{IssueDraft, ProjectTarget};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDraft {
    pub project: ProjectTarget,
    pub value: IssueDraft,
    pub baseline: Option<IssueDraft>,
    #[serde(default)]
    pub warning: Option<String>,
}
pub type TaskDrafts = BTreeMap<String, TaskDraft>;
pub fn valid(drafts: &TaskDrafts) -> bool {
    drafts.len() <= 128
        && drafts.iter().all(|(key, draft)| {
            !key.is_empty()
                && key.len() <= 768
                && draft
                    .warning
                    .as_ref()
                    .is_none_or(|warning| warning.len() <= 4096)
                && draft.project.valid()
                && [&draft.value]
                    .into_iter()
                    .chain(draft.baseline.iter())
                    .all(|v| {
                        v.fields.len() <= 256
                            && serde_json::to_vec(&v.fields).is_ok_and(|b| b.len() <= 1024 * 1024)
                            && v.title.chars().count() <= 4096
                            && v.body.len() <= 1024 * 1024
                            && v.labels.len() <= 32
                            && v.assignees.len() <= 16
                            && v.labels
                                .iter()
                                .chain(v.assignees.iter())
                                .all(|s| s.len() <= 200)
                    })
        })
}
