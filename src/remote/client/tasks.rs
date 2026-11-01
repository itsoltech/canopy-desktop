//! Exact integration requests use the instance-owned mutation journal.
use super::{HostClient, api_commands::ApiIntent};
use crate::api::proto as p;

pub enum TaskMutation {
    Apply(p::ApplyMutationRequest),
    Update(p::UpdateTaskRequest),
    SaveFilter(p::SaveFilterRequest),
    RemoveFilter(p::RemoveFilterRequest),
    Worktree(p::TaskToWorktreeRequest),
}
impl HostClient {
    pub async fn mutate_task(&self, mutation: TaskMutation) -> Result<p::CommandReceipt, String> {
        let intent = match mutation {
            TaskMutation::Apply(r) => ApiIntent::TaskMutation(r),
            TaskMutation::Update(r) => ApiIntent::TaskUpdate(r),
            TaskMutation::SaveFilter(mut r) => {
                r.environment = Some(p::EnvironmentRef {
                    host_id: self.id().as_str().into(),
                });
                ApiIntent::TaskSaveFilter(r)
            }
            TaskMutation::RemoveFilter(mut r) => {
                r.environment = Some(p::EnvironmentRef {
                    host_id: self.id().as_str().into(),
                });
                ApiIntent::TaskRemoveFilter(r)
            }
            TaskMutation::Worktree(r) => ApiIntent::TaskWorktree(r),
        };
        self.mutate_api_typed(intent, String::new(), None).await
    }
}
