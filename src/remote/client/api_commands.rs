//! Exact typed requests share the existing device journal and epoch owner.
use super::{
    HostClient,
    commands::{check_receipt, terminal_state},
    rpc_error,
    store::MutationRecord,
};
use crate::api::proto as p;
use prost::Message;
use tonic::transport::Channel;

#[derive(Clone, Debug)]
pub enum ApiIntent {
    TaskMutation(p::ApplyMutationRequest),
    TaskUpdate(p::UpdateTaskRequest),
    TaskSaveFilter(p::SaveFilterRequest),
    TaskRemoveFilter(p::RemoveFilterRequest),
    TaskWorktree(p::TaskToWorktreeRequest),
    ConnectPeer(p::ConnectPeerRequest),
    RevokeDevice(p::RevokeDeviceRequest),
    SaveAgent(p::SaveAgentDefinitionRequest),
    RemoveAgent(p::RemoveAgentDefinitionRequest),
    SaveProfile(Box<p::SaveProfileRequest>),
    RemoveProfile(p::RemoveProfileRequest),
    SaveIntegration(p::SaveIntegrationRequest),
    RemoveIntegration(p::RemoveIntegrationRequest),
    BeginImage(p::BeginImageUploadRequest),
    FinishImage(p::FinishImageUploadRequest),
    CancelImage(p::CancelImageUploadRequest),
    BeginFile(p::BeginFileUploadRequest),
    FinishFile(p::FinishFileUploadRequest),
    CancelFile(p::CancelFileUploadRequest),
    BeginTask(p::BeginTaskUploadRequest),
    FinishTask(p::FinishTaskUploadRequest),
    CancelTask(p::CancelTaskUploadRequest),
    OpenTerminal(p::OpenTerminalRequest),
    StopTerminal(p::StopTerminalRequest),
    RestartTerminal(p::RestartTerminalRequest),
    AcquireInput(p::AcquireInputRequest),
    ReleaseInput(p::ReleaseInputRequest),
    GitStage(p::StageRequest),
    GitUnstage(p::UnstageRequest),
    GitDiscard(p::DiscardRequest),
    GitCommit(p::CommitRequest),
    GitPull(p::PullRequest),
    GitPush(p::PushRequest),
    GitWorktree(p::ChangeWorktreeRequest),
    SaveFile(p::SaveFileRequest),
    CreateFile(p::CreateFileRequest),
    FilePresence(p::SetDirtyPresenceRequest),
    Create(p::CreateThreadRequest),
    Close(p::CloseThreadRequest),
    CloseProject(p::CloseProjectRequest),
    CloseTerminal(p::CloseTerminalRequest),
    OpenProject(p::OpenProjectRequest),
    Start(p::StartRequest),
    Interrupt(p::InterruptRequest),
    Restart(p::RestartRequest),
    Steer(p::SteerRequest),
    Respond(p::RespondDecisionRequest),
    Option(p::SetAgentOptionRequest),
    Agent(p::SetThreadAgentRequest),
    Goal(p::SetGoalRequest),
    PauseGoal(p::PauseGoalRequest),
    ResumeGoal(p::ResumeGoalRequest),
    ClearGoal(p::ClearGoalRequest),
    Background(p::StopBackgroundRequest),
    MessageEdit(p::EditMessageRequest),
    Shell(p::RunShellRequest),
    StopShell(p::StopShellRequest),
    Fork(p::ForkThreadRequest),
    Rename(p::RenameThreadRequest),
    Send(p::SendMessageRequest),
    Edit(p::EditQueueMessageRequest),
    Move(p::MoveQueueMessageRequest),
    Remove(p::RemoveQueueMessageRequest),
    Pause(p::PauseQueueRequest),
    Resolve(p::ResolveUnknownQueueMessageRequest),
    Preferences(p::UpdatePreferencesRequest),
}

macro_rules! request_field {
    ($value:expr, $field:ident) => {
        match $value {
            ApiIntent::TaskMutation(v) => &mut v.$field,
            ApiIntent::TaskUpdate(v) => &mut v.$field,
            ApiIntent::TaskSaveFilter(v) => &mut v.$field,
            ApiIntent::TaskRemoveFilter(v) => &mut v.$field,
            ApiIntent::TaskWorktree(v) => &mut v.$field,
            ApiIntent::ConnectPeer(v) => &mut v.$field,
            ApiIntent::RevokeDevice(v) => &mut v.$field,
            ApiIntent::SaveAgent(v) => &mut v.$field,
            ApiIntent::RemoveAgent(v) => &mut v.$field,
            ApiIntent::SaveProfile(v) => &mut v.$field,
            ApiIntent::RemoveProfile(v) => &mut v.$field,
            ApiIntent::SaveIntegration(v) => &mut v.$field,
            ApiIntent::RemoveIntegration(v) => &mut v.$field,
            ApiIntent::BeginImage(v) => &mut v.$field,
            ApiIntent::FinishImage(v) => &mut v.$field,
            ApiIntent::CancelImage(v) => &mut v.$field,
            ApiIntent::BeginFile(v) => &mut v.$field,
            ApiIntent::FinishFile(v) => &mut v.$field,
            ApiIntent::CancelFile(v) => &mut v.$field,
            ApiIntent::BeginTask(v) => &mut v.$field,
            ApiIntent::FinishTask(v) => &mut v.$field,
            ApiIntent::CancelTask(v) => &mut v.$field,
            ApiIntent::OpenTerminal(v) => &mut v.$field,
            ApiIntent::StopTerminal(v) => &mut v.$field,
            ApiIntent::RestartTerminal(v) => &mut v.$field,
            ApiIntent::AcquireInput(v) => &mut v.$field,
            ApiIntent::ReleaseInput(v) => &mut v.$field,
            ApiIntent::GitStage(v) => &mut v.$field,
            ApiIntent::GitUnstage(v) => &mut v.$field,
            ApiIntent::GitDiscard(v) => &mut v.$field,
            ApiIntent::GitCommit(v) => &mut v.$field,
            ApiIntent::GitPull(v) => &mut v.$field,
            ApiIntent::GitPush(v) => &mut v.$field,
            ApiIntent::GitWorktree(v) => &mut v.$field,
            ApiIntent::SaveFile(v) => &mut v.$field,
            ApiIntent::CreateFile(v) => &mut v.$field,
            ApiIntent::FilePresence(v) => &mut v.$field,
            ApiIntent::Create(v) => &mut v.$field,
            ApiIntent::Close(v) => &mut v.$field,
            ApiIntent::CloseProject(v) => &mut v.$field,
            ApiIntent::CloseTerminal(v) => &mut v.$field,
            ApiIntent::OpenProject(v) => &mut v.$field,
            ApiIntent::Start(v) => &mut v.$field,
            ApiIntent::Interrupt(v) => &mut v.$field,
            ApiIntent::Restart(v) => &mut v.$field,
            ApiIntent::Steer(v) => &mut v.$field,
            ApiIntent::Respond(v) => &mut v.$field,
            ApiIntent::Option(v) => &mut v.$field,
            ApiIntent::Agent(v) => &mut v.$field,
            ApiIntent::Goal(v) => &mut v.$field,
            ApiIntent::PauseGoal(v) => &mut v.$field,
            ApiIntent::ResumeGoal(v) => &mut v.$field,
            ApiIntent::ClearGoal(v) => &mut v.$field,
            ApiIntent::Background(v) => &mut v.$field,
            ApiIntent::MessageEdit(v) => &mut v.$field,
            ApiIntent::Shell(v) => &mut v.$field,
            ApiIntent::StopShell(v) => &mut v.$field,
            ApiIntent::Fork(v) => &mut v.$field,
            ApiIntent::Rename(v) => &mut v.$field,
            ApiIntent::Send(v) => &mut v.$field,
            ApiIntent::Edit(v) => &mut v.$field,
            ApiIntent::Move(v) => &mut v.$field,
            ApiIntent::Remove(v) => &mut v.$field,
            ApiIntent::Pause(v) => &mut v.$field,
            ApiIntent::Resolve(v) => &mut v.$field,
            ApiIntent::Preferences(v) => &mut v.$field,
        }
    };
}
impl ApiIntent {
    pub(super) fn meta(&mut self) -> &mut Option<p::CommandMeta> {
        request_field!(self, command)
    }
    pub(super) fn encode(&self) -> (&'static str, Vec<u8>) {
        match self {
            Self::TaskMutation(v) => ("tasks.mutate", v.encode_to_vec()),
            Self::TaskUpdate(v) => ("tasks.update", v.encode_to_vec()),
            Self::TaskSaveFilter(v) => ("tasks.save_filter", v.encode_to_vec()),
            Self::TaskRemoveFilter(v) => ("tasks.remove_filter", v.encode_to_vec()),
            Self::TaskWorktree(v) => ("tasks.worktree", v.encode_to_vec()),
            Self::ConnectPeer(v) => ("peer.connect", v.encode_to_vec()),
            Self::RevokeDevice(v) => ("host.revoke", v.encode_to_vec()),
            Self::SaveAgent(v) => ("settings.agent.save", v.encode_to_vec()),
            Self::RemoveAgent(v) => ("settings.agent.remove", v.encode_to_vec()),
            Self::SaveProfile(v) => ("settings.profile.save", v.encode_to_vec()),
            Self::RemoveProfile(v) => ("settings.profile.remove", v.encode_to_vec()),
            Self::SaveIntegration(v) => ("settings.integration.save", v.encode_to_vec()),
            Self::RemoveIntegration(v) => ("settings.integration.remove", v.encode_to_vec()),
            Self::BeginImage(v) => ("artifacts.image.begin", v.encode_to_vec()),
            Self::FinishImage(v) => ("artifacts.image.finish", v.encode_to_vec()),
            Self::CancelImage(v) => ("artifacts.image.cancel", v.encode_to_vec()),
            Self::BeginFile(v) => ("artifacts.file.begin", v.encode_to_vec()),
            Self::FinishFile(v) => ("artifacts.file.finish", v.encode_to_vec()),
            Self::CancelFile(v) => ("artifacts.file.cancel", v.encode_to_vec()),
            Self::BeginTask(v) => ("artifacts.task.begin", v.encode_to_vec()),
            Self::FinishTask(v) => ("artifacts.task.finish", v.encode_to_vec()),
            Self::CancelTask(v) => ("artifacts.task.cancel", v.encode_to_vec()),
            Self::OpenTerminal(v) => ("terminal.open", v.encode_to_vec()),
            Self::StopTerminal(v) => ("terminal.stop", v.encode_to_vec()),
            Self::RestartTerminal(v) => ("terminal.restart", v.encode_to_vec()),
            Self::AcquireInput(v) => ("terminal.acquire", v.encode_to_vec()),
            Self::ReleaseInput(v) => ("terminal.release", v.encode_to_vec()),
            Self::GitStage(v) => ("git.stage", v.encode_to_vec()),
            Self::GitUnstage(v) => ("git.unstage", v.encode_to_vec()),
            Self::GitDiscard(v) => ("git.discard", v.encode_to_vec()),
            Self::GitCommit(v) => ("git.commit", v.encode_to_vec()),
            Self::GitPull(v) => ("git.pull", v.encode_to_vec()),
            Self::GitPush(v) => ("git.push", v.encode_to_vec()),
            Self::GitWorktree(v) => ("git.worktree", v.encode_to_vec()),
            Self::SaveFile(v) => ("files.save", v.encode_to_vec()),
            Self::CreateFile(v) => ("files.create", v.encode_to_vec()),
            Self::FilePresence(v) => ("files.presence", v.encode_to_vec()),
            Self::Create(v) => ("create", v.encode_to_vec()),
            Self::Close(v) => ("close", v.encode_to_vec()),
            Self::OpenProject(v) => ("project.open", v.encode_to_vec()),
            Self::CloseProject(v) => ("project.close", v.encode_to_vec()),
            Self::CloseTerminal(v) => ("terminal.close", v.encode_to_vec()),
            Self::Start(v) => ("start", v.encode_to_vec()),
            Self::Interrupt(v) => ("interrupt", v.encode_to_vec()),
            Self::Restart(v) => ("restart", v.encode_to_vec()),
            Self::Steer(v) => ("steer", v.encode_to_vec()),
            Self::Respond(v) => ("respond", v.encode_to_vec()),
            Self::Option(v) => ("option", v.encode_to_vec()),
            Self::Agent(v) => ("agent", v.encode_to_vec()),
            Self::Goal(v) => ("goal", v.encode_to_vec()),
            Self::PauseGoal(v) => ("goal.pause", v.encode_to_vec()),
            Self::ResumeGoal(v) => ("goal.resume", v.encode_to_vec()),
            Self::ClearGoal(v) => ("goal.clear", v.encode_to_vec()),
            Self::Background(v) => ("background", v.encode_to_vec()),
            Self::MessageEdit(v) => ("message.edit", v.encode_to_vec()),
            Self::Shell(v) => ("shell", v.encode_to_vec()),
            Self::StopShell(v) => ("shell.stop", v.encode_to_vec()),
            Self::Fork(v) => ("fork", v.encode_to_vec()),
            Self::Rename(v) => ("rename", v.encode_to_vec()),
            Self::Send(v) => ("send", v.encode_to_vec()),
            Self::Edit(v) => ("queue.edit", v.encode_to_vec()),
            Self::Move(v) => ("queue.move", v.encode_to_vec()),
            Self::Remove(v) => ("queue.remove", v.encode_to_vec()),
            Self::Pause(v) => ("queue.pause", v.encode_to_vec()),
            Self::Resolve(v) => ("queue.resolve", v.encode_to_vec()),
            Self::Preferences(v) => ("preferences", v.encode_to_vec()),
        }
    }
    pub(super) fn decode(kind: &str, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            "tasks.mutate" => p::ApplyMutationRequest::decode(bytes).map(Self::TaskMutation),
            "tasks.update" => p::UpdateTaskRequest::decode(bytes).map(Self::TaskUpdate),
            "tasks.save_filter" => p::SaveFilterRequest::decode(bytes).map(Self::TaskSaveFilter),
            "tasks.remove_filter" => {
                p::RemoveFilterRequest::decode(bytes).map(Self::TaskRemoveFilter)
            }
            "tasks.worktree" => p::TaskToWorktreeRequest::decode(bytes).map(Self::TaskWorktree),
            "peer.connect" => p::ConnectPeerRequest::decode(bytes).map(Self::ConnectPeer),
            "host.revoke" => p::RevokeDeviceRequest::decode(bytes).map(Self::RevokeDevice),
            "settings.agent.save" => {
                p::SaveAgentDefinitionRequest::decode(bytes).map(Self::SaveAgent)
            }
            "settings.agent.remove" => {
                p::RemoveAgentDefinitionRequest::decode(bytes).map(Self::RemoveAgent)
            }
            "settings.profile.save" => {
                p::SaveProfileRequest::decode(bytes).map(|v| Self::SaveProfile(Box::new(v)))
            }
            "settings.profile.remove" => {
                p::RemoveProfileRequest::decode(bytes).map(Self::RemoveProfile)
            }
            "settings.integration.save" => {
                p::SaveIntegrationRequest::decode(bytes).map(Self::SaveIntegration)
            }
            "settings.integration.remove" => {
                p::RemoveIntegrationRequest::decode(bytes).map(Self::RemoveIntegration)
            }
            "artifacts.image.begin" => {
                p::BeginImageUploadRequest::decode(bytes).map(Self::BeginImage)
            }
            "artifacts.image.finish" => {
                p::FinishImageUploadRequest::decode(bytes).map(Self::FinishImage)
            }
            "artifacts.image.cancel" => {
                p::CancelImageUploadRequest::decode(bytes).map(Self::CancelImage)
            }
            "artifacts.file.begin" => p::BeginFileUploadRequest::decode(bytes).map(Self::BeginFile),
            "artifacts.file.finish" => {
                p::FinishFileUploadRequest::decode(bytes).map(Self::FinishFile)
            }
            "artifacts.file.cancel" => {
                p::CancelFileUploadRequest::decode(bytes).map(Self::CancelFile)
            }
            "artifacts.task.begin" => p::BeginTaskUploadRequest::decode(bytes).map(Self::BeginTask),
            "artifacts.task.finish" => {
                p::FinishTaskUploadRequest::decode(bytes).map(Self::FinishTask)
            }
            "artifacts.task.cancel" => {
                p::CancelTaskUploadRequest::decode(bytes).map(Self::CancelTask)
            }
            "terminal.open" => p::OpenTerminalRequest::decode(bytes).map(Self::OpenTerminal),
            "terminal.stop" => p::StopTerminalRequest::decode(bytes).map(Self::StopTerminal),
            "terminal.restart" => {
                p::RestartTerminalRequest::decode(bytes).map(Self::RestartTerminal)
            }
            "terminal.acquire" => p::AcquireInputRequest::decode(bytes).map(Self::AcquireInput),
            "terminal.release" => p::ReleaseInputRequest::decode(bytes).map(Self::ReleaseInput),
            "git.stage" => p::StageRequest::decode(bytes).map(Self::GitStage),
            "git.unstage" => p::UnstageRequest::decode(bytes).map(Self::GitUnstage),
            "git.discard" => p::DiscardRequest::decode(bytes).map(Self::GitDiscard),
            "git.commit" => p::CommitRequest::decode(bytes).map(Self::GitCommit),
            "git.pull" => p::PullRequest::decode(bytes).map(Self::GitPull),
            "git.push" => p::PushRequest::decode(bytes).map(Self::GitPush),
            "git.worktree" => p::ChangeWorktreeRequest::decode(bytes).map(Self::GitWorktree),
            "files.save" => p::SaveFileRequest::decode(bytes).map(Self::SaveFile),
            "files.create" => p::CreateFileRequest::decode(bytes).map(Self::CreateFile),
            "files.presence" => p::SetDirtyPresenceRequest::decode(bytes).map(Self::FilePresence),
            "create" => p::CreateThreadRequest::decode(bytes).map(Self::Create),
            "close" => p::CloseThreadRequest::decode(bytes).map(Self::Close),
            "project.close" => p::CloseProjectRequest::decode(bytes).map(Self::CloseProject),
            "terminal.close" => p::CloseTerminalRequest::decode(bytes).map(Self::CloseTerminal),
            "project.open" => p::OpenProjectRequest::decode(bytes).map(Self::OpenProject),
            "start" => p::StartRequest::decode(bytes).map(Self::Start),
            "interrupt" => p::InterruptRequest::decode(bytes).map(Self::Interrupt),
            "restart" => p::RestartRequest::decode(bytes).map(Self::Restart),
            "steer" => p::SteerRequest::decode(bytes).map(Self::Steer),
            "respond" => p::RespondDecisionRequest::decode(bytes).map(Self::Respond),
            "option" => p::SetAgentOptionRequest::decode(bytes).map(Self::Option),
            "agent" => p::SetThreadAgentRequest::decode(bytes).map(Self::Agent),
            "goal" => p::SetGoalRequest::decode(bytes).map(Self::Goal),
            "goal.pause" => p::PauseGoalRequest::decode(bytes).map(Self::PauseGoal),
            "goal.resume" => p::ResumeGoalRequest::decode(bytes).map(Self::ResumeGoal),
            "goal.clear" => p::ClearGoalRequest::decode(bytes).map(Self::ClearGoal),
            "background" => p::StopBackgroundRequest::decode(bytes).map(Self::Background),
            "message.edit" => p::EditMessageRequest::decode(bytes).map(Self::MessageEdit),
            "shell" => p::RunShellRequest::decode(bytes).map(Self::Shell),
            "shell.stop" => p::StopShellRequest::decode(bytes).map(Self::StopShell),
            "fork" => p::ForkThreadRequest::decode(bytes).map(Self::Fork),
            "rename" => p::RenameThreadRequest::decode(bytes).map(Self::Rename),
            "send" => p::SendMessageRequest::decode(bytes).map(Self::Send),
            "queue.edit" => p::EditQueueMessageRequest::decode(bytes).map(Self::Edit),
            "queue.move" => p::MoveQueueMessageRequest::decode(bytes).map(Self::Move),
            "queue.remove" => p::RemoveQueueMessageRequest::decode(bytes).map(Self::Remove),
            "queue.pause" => p::PauseQueueRequest::decode(bytes).map(Self::Pause),
            "queue.resolve" => {
                p::ResolveUnknownQueueMessageRequest::decode(bytes).map(Self::Resolve)
            }
            "preferences" => p::UpdatePreferencesRequest::decode(bytes).map(Self::Preferences),
            _ => return Err("Unknown saved API request; intent retained.".into()),
        }
        .map_err(|_| "Invalid saved API request; intent retained.".into())
    }
    pub(super) fn resource(&self) -> Option<p::command_receipt::Resource> {
        let task = match self {
            Self::TaskMutation(v) => v.project.as_ref(),
            Self::TaskUpdate(v) => v.task.as_ref().and_then(|t| t.project.as_ref()),
            Self::TaskWorktree(v) => v.task.as_ref().and_then(|t| t.project.as_ref()),
            _ => None,
        };
        if let Some(task) = task {
            return task.resource.clone().map(|resource| match resource {
                p::task_project::Resource::Project(p) => p::command_receipt::Resource::Project(p),
                p::task_project::Resource::Environment(e) => {
                    p::command_receipt::Resource::Environment(e)
                }
            });
        }
        let environment = match self {
            Self::TaskSaveFilter(v) => Some(&v.environment),
            Self::TaskRemoveFilter(v) => Some(&v.environment),
            Self::ConnectPeer(v) => Some(&v.environment),
            Self::RevokeDevice(v) => Some(&v.environment),
            Self::SaveAgent(v) => Some(&v.environment),
            Self::RemoveAgent(v) => Some(&v.environment),
            Self::SaveProfile(v) => Some(&v.environment),
            Self::RemoveProfile(v) => Some(&v.environment),
            Self::SaveIntegration(v) => Some(&v.environment),
            Self::RemoveIntegration(v) => Some(&v.environment),
            _ => None,
        };
        if let Some(environment) = environment {
            return environment
                .clone()
                .map(p::command_receipt::Resource::Environment);
        }
        let project = match self {
            Self::CloseProject(v) => Some(&v.project),
            Self::CloseTerminal(v) => Some(&v.project),
            Self::OpenTerminal(v) => Some(&v.project),
            Self::StopTerminal(v) => Some(&v.project),
            Self::RestartTerminal(v) => Some(&v.project),
            Self::AcquireInput(v) => Some(&v.project),
            Self::ReleaseInput(v) => Some(&v.project),
            Self::GitStage(v) => Some(&v.project),
            Self::GitUnstage(v) => Some(&v.project),
            Self::GitDiscard(v) => Some(&v.project),
            Self::GitCommit(v) => Some(&v.project),
            Self::GitPull(v) => Some(&v.project),
            Self::GitPush(v) => Some(&v.project),
            Self::GitWorktree(v) => Some(&v.project),
            _ => None,
        };
        if let Some(project) = project {
            return project.clone().map(p::command_receipt::Resource::Project);
        }
        let file = match self {
            Self::BeginFile(v) => v.file.as_ref(),
            Self::FinishFile(v) => v.file.as_ref(),
            Self::CancelFile(v) => v.file.as_ref(),
            Self::SaveFile(v) => v.file.as_ref(),
            Self::CreateFile(v) => v.file.as_ref(),
            Self::FilePresence(v) => v.file.as_ref(),
            _ => None,
        };
        if let Some(file) = file {
            return file.target.as_ref()?.resource.clone().map(|r| match r {
                p::resource_target::Resource::Project(p) => {
                    p::command_receipt::Resource::Project(p)
                }
                p::resource_target::Resource::Thread(t) => p::command_receipt::Resource::Target(t),
            });
        }
        let task = match self {
            Self::BeginTask(v) => v.task.as_ref(),
            Self::FinishTask(v) => v.task.as_ref(),
            Self::CancelTask(v) => v.task.as_ref(),
            _ => None,
        };
        if let Some(task) = task {
            return task.project.as_ref()?.resource.clone().map(|r| match r {
                p::task_project::Resource::Project(p) => p::command_receipt::Resource::Project(p),
                p::task_project::Resource::Environment(e) => {
                    p::command_receipt::Resource::Environment(e)
                }
            });
        }
        let target = match self {
            Self::ConnectPeer(_)
            | Self::RevokeDevice(_)
            | Self::SaveAgent(_)
            | Self::RemoveAgent(_)
            | Self::SaveProfile(_)
            | Self::RemoveProfile(_)
            | Self::SaveIntegration(_)
            | Self::RemoveIntegration(_) => return None,
            Self::BeginFile(_)
            | Self::FinishFile(_)
            | Self::CancelFile(_)
            | Self::BeginTask(_)
            | Self::FinishTask(_)
            | Self::CancelTask(_) => return None,
            Self::BeginImage(v) => &v.target,
            Self::FinishImage(v) => &v.target,
            Self::CancelImage(v) => &v.target,
            Self::SaveFile(_)
            | Self::CreateFile(_)
            | Self::FilePresence(_)
            | Self::GitStage(_)
            | Self::GitUnstage(_)
            | Self::GitDiscard(_)
            | Self::GitCommit(_)
            | Self::GitPull(_)
            | Self::GitPush(_)
            | Self::GitWorktree(_) => return None,
            Self::OpenTerminal(_)
            | Self::CloseProject(_)
            | Self::CloseTerminal(_)
            | Self::StopTerminal(_)
            | Self::RestartTerminal(_)
            | Self::AcquireInput(_)
            | Self::ReleaseInput(_) => return None,
            Self::Close(v) => &v.target,
            Self::Create(v) => {
                return v.resource.clone().map(|r| match r {
                    p::create_thread_request::Resource::Project(p) => {
                        p::command_receipt::Resource::Project(p)
                    }
                    p::create_thread_request::Resource::Environment(e) => {
                        p::command_receipt::Resource::Environment(e)
                    }
                });
            }
            Self::OpenProject(v) => {
                return v
                    .environment
                    .clone()
                    .map(p::command_receipt::Resource::Environment);
            }
            Self::TaskMutation(_)
            | Self::TaskUpdate(_)
            | Self::TaskWorktree(_)
            | Self::TaskSaveFilter(_)
            | Self::TaskRemoveFilter(_) => return None,
            Self::Start(v) => &v.target,
            Self::Interrupt(v) => &v.target,
            Self::Restart(v) => &v.target,
            Self::Steer(v) => &v.target,
            Self::Respond(v) => &v.target,
            Self::Option(v) => &v.target,
            Self::Agent(v) => &v.target,
            Self::Goal(v) => &v.target,
            Self::PauseGoal(v) => &v.target,
            Self::ResumeGoal(v) => &v.target,
            Self::ClearGoal(v) => &v.target,
            Self::Background(v) => &v.target,
            Self::MessageEdit(v) => &v.target,
            Self::Shell(v) => &v.target,
            Self::StopShell(v) => &v.target,
            Self::Fork(v) => &v.target,
            Self::Rename(v) => &v.target,
            Self::Send(v) => &v.target,
            Self::Edit(v) => &v.target,
            Self::Move(v) => &v.target,
            Self::Remove(v) => &v.target,
            Self::Pause(v) => &v.target,
            Self::Resolve(v) => &v.target,
            Self::Preferences(v) => {
                return v
                    .environment
                    .clone()
                    .map(p::command_receipt::Resource::Environment);
            }
        };
        target.clone().map(p::command_receipt::Resource::Target)
    }
    pub(super) async fn dispatch(
        self,
        channel: Channel,
        host: &str,
        _catalog_id: String,
    ) -> Result<p::CommandReceipt, String> {
        self.dispatch_typed(channel, host).await
    }
    pub(super) async fn dispatch_typed(
        mut self,
        channel: Channel,
        host: &str,
    ) -> Result<p::CommandReceipt, String> {
        let meta = self
            .meta()
            .clone()
            .ok_or("Saved API command identity is missing.")?;
        let resource = self.resource().ok_or("Saved API target is missing.")?;
        let requested_host = match &resource {
            p::command_receipt::Resource::Target(target) => {
                let thread = target
                    .thread
                    .as_ref()
                    .ok_or("Thread identity is missing.")?;
                if crate::thread::ThreadId::parse(&thread.thread_id).is_none() {
                    return Err("Invalid saved thread identity.".into());
                }
                &thread.host_id
            }
            p::command_receipt::Resource::Environment(e) => &e.host_id,
            p::command_receipt::Resource::Project(p) => &p.host_id,
        };
        if requested_host != host {
            return Err("Saved API request belongs to another host.".into());
        }
        let mut thread = p::thread_service_client::ThreadServiceClient::new(channel.clone());
        let result = match self {
            Self::TaskMutation(v) => {
                p::tasks_service_client::TasksServiceClient::new(channel)
                    .apply_mutation(v)
                    .await
            }
            Self::TaskUpdate(v) => {
                p::tasks_service_client::TasksServiceClient::new(channel)
                    .update_task(v)
                    .await
            }
            Self::TaskSaveFilter(v) => {
                p::tasks_service_client::TasksServiceClient::new(channel)
                    .save_filter(v)
                    .await
            }
            Self::TaskRemoveFilter(v) => {
                p::tasks_service_client::TasksServiceClient::new(channel)
                    .remove_filter(v)
                    .await
            }
            Self::TaskWorktree(v) => {
                p::tasks_service_client::TasksServiceClient::new(channel)
                    .task_to_worktree(v)
                    .await
            }
            Self::ConnectPeer(v) => {
                p::peer_service_client::PeerServiceClient::new(channel)
                    .connect_peer(v)
                    .await
            }
            Self::RevokeDevice(v) => {
                p::host_service_client::HostServiceClient::new(channel)
                    .revoke_device(v)
                    .await
            }
            Self::SaveAgent(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .save_agent_definition(v)
                    .await
            }
            Self::RemoveAgent(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .remove_agent_definition(v)
                    .await
            }
            Self::SaveProfile(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .save_profile(*v)
                    .await
            }
            Self::RemoveProfile(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .remove_profile(v)
                    .await
            }
            Self::SaveIntegration(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .save_integration(v)
                    .await
            }
            Self::RemoveIntegration(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .remove_integration(v)
                    .await
            }
            Self::BeginImage(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .begin_image_upload(v)
                    .await
            }
            Self::FinishImage(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .finish_image_upload(v)
                    .await
            }
            Self::CancelImage(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .cancel_image_upload(v)
                    .await
            }
            Self::BeginFile(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .begin_file_upload(v)
                    .await
            }
            Self::FinishFile(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .finish_file_upload(v)
                    .await
            }
            Self::CancelFile(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .cancel_file_upload(v)
                    .await
            }
            Self::BeginTask(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .begin_task_upload(v)
                    .await
            }
            Self::FinishTask(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .finish_task_upload(v)
                    .await
            }
            Self::CancelTask(v) => {
                p::artifacts_service_client::ArtifactsServiceClient::new(channel)
                    .cancel_task_upload(v)
                    .await
            }
            Self::OpenTerminal(v) => {
                p::terminal_service_client::TerminalServiceClient::new(channel)
                    .open_terminal(v)
                    .await
            }
            Self::StopTerminal(v) => {
                p::terminal_service_client::TerminalServiceClient::new(channel)
                    .stop_terminal(v)
                    .await
            }
            Self::RestartTerminal(v) => {
                p::terminal_service_client::TerminalServiceClient::new(channel)
                    .restart_terminal(v)
                    .await
            }
            Self::AcquireInput(v) => {
                p::terminal_service_client::TerminalServiceClient::new(channel)
                    .acquire_input(v)
                    .await
            }
            Self::ReleaseInput(v) => {
                p::terminal_service_client::TerminalServiceClient::new(channel)
                    .release_input(v)
                    .await
            }
            Self::GitStage(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .stage(v)
                    .await
            }
            Self::GitUnstage(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .unstage(v)
                    .await
            }
            Self::GitDiscard(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .discard(v)
                    .await
            }
            Self::GitCommit(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .commit(v)
                    .await
            }
            Self::GitPull(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .pull(v)
                    .await
            }
            Self::GitPush(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .push(v)
                    .await
            }
            Self::GitWorktree(v) => {
                p::git_service_client::GitServiceClient::new(channel)
                    .change_worktree(v)
                    .await
            }
            Self::SaveFile(v) => {
                p::files_service_client::FilesServiceClient::new(channel)
                    .save_file(v)
                    .await
            }
            Self::CreateFile(v) => {
                p::files_service_client::FilesServiceClient::new(channel)
                    .create_file(v)
                    .await
            }
            Self::FilePresence(v) => {
                p::files_service_client::FilesServiceClient::new(channel)
                    .set_dirty_presence(v)
                    .await
            }
            Self::Create(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .create_thread(v)
                    .await
            }
            Self::Close(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .close_thread(v)
                    .await
            }
            Self::OpenProject(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .open_project(v)
                    .await
            }
            Self::CloseProject(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .close_project(v)
                    .await
            }
            Self::CloseTerminal(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .close_terminal(v)
                    .await
            }
            Self::Start(v) => thread.start(v).await,
            Self::Interrupt(v) => thread.interrupt(v).await,
            Self::Restart(v) => thread.restart(v).await,
            Self::Steer(v) => thread.steer(v).await,
            Self::Respond(v) => thread.respond_decision(v).await,
            Self::Option(v) => thread.set_agent_option(v).await,
            Self::Agent(v) => thread.set_thread_agent(v).await,
            Self::Goal(v) => thread.set_goal(v).await,
            Self::PauseGoal(v) => thread.pause_goal(v).await,
            Self::ResumeGoal(v) => thread.resume_goal(v).await,
            Self::ClearGoal(v) => thread.clear_goal(v).await,
            Self::Background(v) => thread.stop_background(v).await,
            Self::MessageEdit(v) => thread.edit_message(v).await,
            Self::Shell(v) => thread.run_shell(v).await,
            Self::StopShell(v) => thread.stop_shell(v).await,
            Self::Fork(v) => thread.fork_thread(v).await,
            Self::Rename(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .rename_thread(v)
                    .await
            }
            Self::Send(v) => thread.send_message(v).await,
            Self::Edit(v) => thread.edit_queue_message(v).await,
            Self::Move(v) => thread.move_queue_message(v).await,
            Self::Remove(v) => thread.remove_queue_message(v).await,
            Self::Pause(v) => thread.pause_queue(v).await,
            Self::Resolve(v) => thread.resolve_unknown_queue_message(v).await,
            Self::Preferences(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .update_preferences(v)
                    .await
            }
        }
        .map_err(rpc_error)?
        .into_inner();
        if result.host_id != host
            || result.command_id != meta.command_id
            || result.command_epoch != meta.command_epoch
            || result.resource.as_ref() != Some(&resource)
        {
            return Err("API result identity differs from the original request; retain intent for reconciliation.".into());
        }
        Ok(result)
    }
}

impl HostClient {
    pub async fn start_thread(&self, chat: String) -> Result<p::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        self.mutate_api(
            ApiIntent::Start(p::StartRequest {
                command: None,
                target: Some(target),
            }),
            chat,
            None,
        )
        .await
    }
    pub async fn list_threads(&self) -> Result<Vec<p::ThreadSummary>, String> {
        let mut threads = Vec::new();
        let mut page_token = None;
        let mut bytes = 0;
        loop {
            let response = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
                .list_threads(p::ListThreadsRequest {
                    environment: Some(p::EnvironmentRef {
                        host_id: self.id().as_str().into(),
                    }),
                    catalog_id: None,
                    page_token: page_token.clone(),
                    limit: 100,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
            bytes += response.encoded_len();
            if bytes > super::catalog::MAX_CATALOG_BYTES {
                return Err("Thread catalog exceeds the client budget.".into());
            }
            if response
                .next_page_token
                .as_ref()
                .zip(page_token.as_ref())
                .is_some_and(|(next, previous)| next <= previous)
            {
                return Err("Thread catalog pagination did not advance.".into());
            }
            threads.extend(response.threads);
            let Some(next) = response.next_page_token else {
                return Ok(threads);
            };
            page_token = Some(next);
        }
    }
    /// Discovery returns the stored UUID and epoch. A legacy key is never parsed as UUIDv7.
    pub async fn thread_target(&self, catalog_id: &str) -> Result<p::ThreadTarget, String> {
        let mut request = tonic::Request::new(p::ListThreadsRequest {
            environment: Some(p::EnvironmentRef {
                host_id: self.id().as_str().into(),
            }),
            catalog_id: Some(catalog_id.into()),
            page_token: None,
            limit: 1,
        });
        request.set_timeout(std::time::Duration::from_secs(15));
        let response = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_threads(request)
            .await
            .map_err(rpc_error)?
            .into_inner();
        let mut matching = response
            .threads
            .into_iter()
            .filter(|t| t.catalog_id == catalog_id);
        let summary = matching
            .next()
            .ok_or("Thread is no longer available on its original host.")?;
        if matching.next().is_some() {
            return Err("Thread discovery is ambiguous.".into());
        }
        let target = summary
            .target
            .ok_or("Thread discovery omitted its address.")?;
        let thread = target
            .thread
            .as_ref()
            .ok_or("Thread identity is missing.")?;
        if thread.host_id != self.id().as_str()
            || crate::thread::ThreadId::parse(&thread.thread_id).is_none()
        {
            return Err("Invalid discovered thread identity.".into());
        }
        Ok(target)
    }
    pub(super) async fn api_send(
        &self,
        request: &p::SendMessageRequest,
    ) -> Result<ApiIntent, String> {
        if request
            .target
            .as_ref()
            .and_then(|t| t.thread.as_ref())
            .is_none_or(|t| t.host_id != self.id().as_str())
        {
            return Err("Thread address is missing or belongs to another host.".into());
        }
        Ok(ApiIntent::Send(request.clone()))
    }
    pub(super) async fn mutate_api(
        &self,
        intent: ApiIntent,
        catalog_id: String,
        revision: Option<u64>,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api_typed(intent, catalog_id, revision).await
    }
    pub(super) async fn mutate_api_typed(
        &self,
        intent: ApiIntent,
        catalog_id: String,
        revision: Option<u64>,
    ) -> Result<p::CommandReceipt, String> {
        let host = self.id().as_str().to_owned();
        self.mutate_api_with(
            intent,
            catalog_id,
            revision,
            move |intent, channel| async move { intent.dispatch_typed(channel, &host).await },
        )
        .await
    }
    pub(super) async fn mutate_api_with<F, Fut>(
        &self,
        mut intent: ApiIntent,
        catalog_id: String,
        revision: Option<u64>,
        dispatch: F,
    ) -> Result<p::CommandReceipt, String>
    where
        F: FnOnce(ApiIntent, Channel) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        let meta = self.command(revision)?;
        *intent.meta() = Some(meta.clone());
        let (kind, bytes) = intent.encode();
        if bytes.len() > crate::host::CONTROL_BYTES {
            return Err("Operation exceeds the control limit; intent retained.".into());
        }
        let old_entries = self.snapshot().record.mutations;
        let bytes = self.save_settings_intent(kind, bytes)?;
        let journaled = self.update_record(|record| {
            if record.mutations.iter().any(|e| e.receipt.as_ref().and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok()).is_none_or(|r| !r.resolved_by_user && !terminal_state(r.state))) {
                return Err("An earlier operation is unresolved. Review its result before another mutation.".into());
            }
            // Every journaled operation is resolved here; only the new one remains.
            record.mutations.clear();
            record.mutations.push(MutationRecord { id: meta.command_id.clone(), epoch: meta.command_epoch, receipt: None, chat_intent: None, api_intent: Some((kind.into(), bytes.clone())) });
            Ok(())
        });
        if let Err(error) = journaled {
            self.remove_settings_intent(kind, &bytes);
            return Err(error);
        }
        for entry in old_entries {
            if let Some((kind, bytes)) = entry.api_intent {
                self.remove_settings_intent(&kind, &bytes);
            }
        }
        let started = std::time::Instant::now();
        let response = dispatch(intent, channel).await;
        crate::remote::metrics::record(
            self.id().as_str(),
            &crate::remote::metrics::resource_id(&catalog_id),
            Some(&meta.command_id),
            0,
            crate::remote::metrics::Stage::ClientRoundTrip,
            started,
            if response.is_ok() {
                crate::remote::metrics::Outcome::Ok
            } else {
                crate::remote::metrics::Outcome::DeliveryUnknown
            },
        );
        let receipt = response?;
        self.store_api_receipt(&meta.command_id, &receipt)?;
        check_receipt(receipt.clone())?;
        Ok(receipt)
    }
    pub(super) fn store_api_receipt(
        &self,
        id: &str,
        receipt: &p::CommandReceipt,
    ) -> Result<(), String> {
        self.update_record(|record| {
            let entry = record
                .mutations
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or("Operation journal disappeared.")?;
            entry.receipt = Some(receipt.encode_to_vec());
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub async fn update_preferences(
        &self,
        mut request: p::UpdatePreferencesRequest,
        revision: u64,
    ) -> Result<p::CommandReceipt, String> {
        request.environment = Some(p::EnvironmentRef {
            host_id: self.id().as_str().into(),
        });
        self.mutate_api(
            ApiIntent::Preferences(request),
            "private".into(),
            Some(revision),
        )
        .await
    }
    pub async fn rename_thread(
        &self,
        chat: String,
        title: String,
        revision: Option<u64>,
    ) -> Result<p::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        self.mutate_api(
            ApiIntent::Rename(p::RenameThreadRequest {
                command: None,
                target: Some(target),
                title,
            }),
            chat,
            revision,
        )
        .await
    }
    pub(super) async fn retry_api(
        &self,
        entry: &MutationRecord,
        catalog_id: String,
    ) -> Result<p::CommandReceipt, String> {
        if self.snapshot().record.epoch != Some(entry.epoch) {
            return Err("Command epoch retired; intent retained without replay.".into());
        }
        if let Some(receipt) = entry
            .receipt
            .as_ref()
            .map(|bytes| p::CommandReceipt::decode(bytes.as_slice()))
            .transpose()
            .map_err(|_| "Invalid saved operation result.")?
        {
            if receipt.resolved_by_user || terminal_state(receipt.state) {
                return Ok(receipt);
            }
            if receipt.state == p::OperationState::DeliveryUnknown as i32
                || receipt.state == p::OperationState::Attempting as i32
            {
                return Err("This operation may already have taken effect. Inspect or review its result; its request cannot be replayed.".into());
            }
        }
        let (kind, bytes) = entry
            .api_intent
            .as_ref()
            .ok_or("Saved typed request is missing.")?;
        let mut intent = self.load_settings_intent(kind, bytes)?;
        let meta = intent
            .meta()
            .as_ref()
            .ok_or("Saved command identity is missing.")?;
        if meta.command_id != entry.id || meta.command_epoch != entry.epoch {
            return Err("Saved API command identity mismatch.".into());
        }
        intent
            .dispatch(self.channel()?, self.id().as_str(), catalog_id)
            .await
    }
}
