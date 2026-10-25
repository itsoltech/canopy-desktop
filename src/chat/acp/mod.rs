//! ACP v1 session controller. The runtime owns this state and one `Process`.
//! It writes returned JSON lines and resolves `take_host_requests` through host
//! services. No file access, terminal creation, or network I/O occurs here.
pub mod wire;

use super::{
    agent::{Capabilities, OptionChoice, OptionEffect, SessionOption},
    backend::Receipt,
    config::CommandInfo,
    process::Event,
    session::{Draft, Status},
    transcript::*,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};
use wire::{Message, RequestId};

const MAX_PENDING: usize = 128;
const MAX_REPLAY_BYTES: usize = 32 * 1024 * 1024;

/// Enable only services the host has installed and qualified for this run.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct HostCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
    pub terminal: bool,
}
impl HostCapabilities {
    pub fn files(mut self, read: bool, write: bool) -> Self {
        self.read_text_file = read;
        self.write_text_file = write;
        self
    }
    pub fn terminal(mut self, enabled: bool) -> Self {
        self.terminal = enabled;
        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum HostOperation {
    #[serde(rename = "fs/read_text_file")]
    ReadTextFile {
        path: String,
        line: Option<u32>,
        limit: Option<u32>,
    },
    #[serde(rename = "fs/write_text_file")]
    WriteTextFile { path: String, content: String },
    #[serde(rename = "terminal/create")]
    CreateTerminal {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: Vec<EnvironmentVariable>,
        cwd: Option<String>,
        #[serde(rename = "outputByteLimit")]
        output_byte_limit: Option<u64>,
    },
    #[serde(rename = "terminal/output")]
    TerminalOutput {
        #[serde(rename = "terminalId")]
        terminal_id: String,
    },
    #[serde(rename = "terminal/wait_for_exit")]
    WaitForExit {
        #[serde(rename = "terminalId")]
        terminal_id: String,
    },
    #[serde(rename = "terminal/kill")]
    KillTerminal {
        #[serde(rename = "terminalId")]
        terminal_id: String,
    },
    #[serde(rename = "terminal/release")]
    ReleaseTerminal {
        #[serde(rename = "terminalId")]
        terminal_id: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct HostRequest {
    pub id: RequestId,
    pub session_id: String,
    pub operation: HostOperation,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: String,
}
impl PermissionOption {
    pub fn new(option_id: String, name: String, kind: String) -> Self {
        Self {
            option_id,
            name,
            kind,
        }
    }
}
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PermissionRequest {
    pub id: RequestId,
    pub item: ItemId,
    pub options: Vec<PermissionOption>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct PlanEntry {
    pub content: String,
    pub priority: String,
    pub status: String,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Start {
    pub cwd: PathBuf,
    pub resume: Option<String>,
    pub checkpoint: Vec<Item>,
    /// True also for a paged checkpoint whose current page happens to be empty.
    pub has_checkpoint: bool,
    pub host: HostCapabilities,
}
impl Start {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            resume: None,
            checkpoint: vec![],
            has_checkpoint: false,
            host: HostCapabilities::default(),
        }
    }
    pub fn resume(mut self, id: String, checkpoint: Vec<Item>) -> Self {
        self.resume = Some(id);
        self.has_checkpoint = true;
        self.checkpoint = checkpoint;
        self
    }
    pub fn with_host(mut self, host: HostCapabilities) -> Self {
        self.host = host;
        self
    }
}

enum Pending {
    Initialize,
    Open,
    Prompt,
    Config,
    Mode(String),
    Model(String),
}

pub struct AcpSession {
    start: Start,
    status: Status,
    session_id: Option<String>,
    capabilities: Capabilities,
    transcript: Transcript,
    replay: Option<Transcript>,
    replay_bytes: usize,
    next_request: u64,
    pending: HashMap<RequestId, Pending>,
    prompt: Option<RequestId>,
    permissions: HashMap<RequestId, PermissionRequest>,
    host_requests: VecDeque<HostRequest>,
    host_pending: HashMap<RequestId, ()>,
    options: Vec<SessionOption>,
    modes: Vec<OptionChoice>,
    current_mode: Option<String>,
    models: Vec<OptionChoice>,
    current_model: Option<String>,
    commands: Vec<CommandInfo>,
    commands_received: bool,
    plan: Vec<PlanEntry>,
    tools: HashMap<String, usize>,
    text: Option<(usize, bool)>,
    turn_start: usize,
    plan_item: Option<usize>,
    replay_user: Option<usize>,
    diagnostics: VecDeque<String>,
    /// The dispatched message waiting for evidence that the agent took it.
    awaiting: Option<String>,
    receipts: Vec<(String, Receipt)>,
}
impl AcpSession {
    pub fn new(mut start: Start) -> Result<(Self, Vec<Value>), String> {
        if !start.cwd.is_absolute() {
            return Err("ACP working directory must be absolute.".into());
        }
        if start.resume.as_ref().is_some_and(|id| id.is_empty()) {
            return Err("ACP session ID is empty.".into());
        }
        let transcript = Transcript::restore(std::mem::take(&mut start.checkpoint));
        let host = start.host;
        let mut this = Self {
            start,
            status: Status::Starting,
            session_id: None,
            capabilities: Capabilities::default(),
            transcript,
            replay: None,
            replay_bytes: 0,
            next_request: 0,
            pending: HashMap::new(),
            prompt: None,
            permissions: HashMap::new(),
            host_requests: VecDeque::new(),
            host_pending: HashMap::new(),
            options: vec![],
            modes: vec![],
            current_mode: None,
            models: vec![],
            current_model: None,
            commands: vec![],
            commands_received: false,
            plan: vec![],
            tools: HashMap::new(),
            text: None,
            turn_start: 0,
            plan_item: None,
            replay_user: None,
            diagnostics: VecDeque::new(),
            awaiting: None,
            receipts: vec![],
        };
        let line = this.request("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"canopy","version":env!("CARGO_PKG_VERSION")},
            "clientCapabilities":{"fs":{"readTextFile":host.read_text_file,"writeTextFile":host.write_text_file},"terminal":host.terminal}}), Pending::Initialize);
        Ok((this, vec![line]))
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }
    /// A host warning belongs to the transcript and follows its normal change feed.
    pub fn note(&mut self, kind: NoticeKind, message: impl Into<String>) {
        self.transcript.push(
            None,
            Body::Notice(Notice {
                kind,
                text: message.into(),
                until: None,
            }),
        );
    }
    pub fn take_changes(&mut self) -> Vec<Change> {
        self.transcript.take_changes()
    }
    pub fn options(&self) -> &[SessionOption] {
        &self.options
    }
    pub fn models(&self) -> &[OptionChoice] {
        &self.models
    }
    pub fn modes(&self) -> &[OptionChoice] {
        &self.modes
    }
    pub fn current_model(&self) -> Option<&str> {
        self.current_model.as_deref()
    }
    pub fn current_mode(&self) -> Option<&str> {
        self.current_mode.as_deref()
    }
    /// ACP has no command-list request. Until the peer sends its first update,
    /// discovery must expose incomplete freshness rather than invent an empty list.
    pub fn catalog_ready(&self) -> bool {
        matches!(self.status, Status::Ready) && self.commands_received
    }
    pub fn commands(&self) -> &[CommandInfo] {
        &self.commands
    }
    pub fn plan(&self) -> &[PlanEntry] {
        &self.plan
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = &str> {
        self.diagnostics.iter().map(String::as_str)
    }
    pub fn permissions(&self) -> impl Iterator<Item = &PermissionRequest> {
        self.permissions.values()
    }
    /// A configuration, model or mode change waits for the agent's answer.
    pub fn config_pending(&self) -> bool {
        self.pending
            .values()
            .any(|p| matches!(p, Pending::Config | Pending::Model(_) | Pending::Mode(_)))
    }
    /// Evidence about dispatched messages since the last call.
    pub fn take_receipts(&mut self) -> Vec<(String, Receipt)> {
        std::mem::take(&mut self.receipts)
    }
    /// A durably accepted message can start a turn now.
    pub fn ready_for_message(&self) -> bool {
        self.status == Status::Ready && self.prompt.is_none() && self.awaiting.is_none()
    }
    fn receipt(&mut self, receipt: Receipt) {
        if let Some(id) = self.awaiting.take() {
            self.receipts.push((id, receipt));
        }
    }
    pub fn take_host_requests(&mut self) -> Vec<HostRequest> {
        self.host_requests.drain(..).collect()
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) -> Value {
        self.next_request += 1;
        let id = RequestId::String(format!("canopy-{}", self.next_request));
        self.pending.insert(id.clone(), pending);
        wire::request(&id, method, params)
    }
    fn ready(&self) -> Result<&str, String> {
        if self.status != Status::Ready {
            return Err("ACP session is not ready.".into());
        }
        self.session_id
            .as_deref()
            .ok_or_else(|| "ACP session has no ID.".into())
    }
    /// Called once after durable dispatch acceptance. Errors leave the draft untouched.
    /// A lost process response is delivery-unknown; this method never retries it.
    pub fn submit(&mut self, draft: &Draft, message_id: &str) -> Result<Vec<Value>, String> {
        let session = self.ready()?.to_owned();
        if self.prompt.is_some() || self.awaiting.is_some() {
            return Err("The agent is busy.".into());
        }
        if draft.goal {
            return Err("This ACP backend does not support goals.".into());
        }
        let mut content = vec![];
        let text = super::compose::plain(&draft.text, &draft.attachments);
        let text = if let Some(context) = &draft.context {
            format!("{context}\n\n{text}")
        } else {
            text
        };
        if !text.is_empty() {
            content.push(json!({"type":"text","text":text}));
        }
        for attachment in &draft.attachments {
            if let Attachment::Image {
                media_type, data, ..
            } = attachment
            {
                if !self.capabilities.images {
                    return Err("This ACP agent does not support images.".into());
                }
                let data = data
                    .as_deref()
                    .ok_or("Image content is not available on the host.")?;
                content.push(json!({"type":"image","mimeType":media_type,"data":data}));
            }
        }
        if content.is_empty() {
            return Err("Message is empty.".into());
        }
        let line = self.request(
            "session/prompt",
            json!({"sessionId":session,"prompt":content}),
            Pending::Prompt,
        );
        self.prompt = serde_json::from_value(line["id"].clone()).ok();
        self.turn_start = self.transcript.items().len();
        self.plan_item = None;
        let turn = self.transcript.allocate_turn();
        self.transcript.set_current_turn(turn);
        self.transcript.push(
            None,
            Body::User(UserMessage {
                text: draft.text.clone(),
                delivery: Delivery::Sent,
                uuid: Some(message_id.into()),
                attachments: draft.attachments.clone(),
                context: draft.context.clone(),
                goal: false,
            }),
        );
        self.text = None;
        self.tools.clear();
        self.plan.clear();
        self.status = Status::Working;
        self.awaiting = Some(message_id.into());
        Ok(vec![line])
    }
    pub fn set_option(&mut self, id: &str, value: &str) -> Result<Vec<Value>, String> {
        let session = self.ready()?.to_owned();
        if self
            .pending
            .values()
            .any(|p| matches!(p, Pending::Config | Pending::Mode(_) | Pending::Model(_)))
        {
            return Err("A configuration change is pending.".into());
        }
        let option = self
            .options
            .iter()
            .find(|option| option.id == id)
            .ok_or("Unknown configuration option.")?;
        if !option.choices.iter().any(|choice| choice.id == value) {
            return Err("Unknown configuration value.".into());
        }
        Ok(vec![self.request(
            "session/set_config_option",
            json!({"sessionId":session,"configId":id,"value":value}),
            Pending::Config,
        )])
    }
    pub fn set_model(&mut self, id: &str) -> Result<Vec<Value>, String> {
        if let Some(option) = self
            .options
            .iter()
            .find(|o| o.category.as_deref() == Some("model"))
        {
            let key = option.id.clone();
            return self.set_option(&key, id);
        }
        let session = self.ready()?.to_owned();
        if !self.models.iter().any(|m| m.id == id) {
            return Err("Model is not offered by this session.".into());
        }
        if self
            .pending
            .values()
            .any(|p| matches!(p, Pending::Model(_)))
        {
            return Err("Model change is pending.".into());
        }
        Ok(vec![self.request(
            "session/set_model",
            json!({"sessionId":session,"modelId":id}),
            Pending::Model(id.into()),
        )])
    }
    pub fn set_mode(&mut self, id: &str) -> Result<Vec<Value>, String> {
        if let Some(option) = self
            .options
            .iter()
            .find(|o| o.category.as_deref() == Some("mode"))
        {
            let key = option.id.clone();
            return self.set_option(&key, id);
        }
        let session = self.ready()?.to_owned();
        if !self.modes.iter().any(|m| m.id == id) {
            return Err("Mode is not offered by this session.".into());
        }
        if self.pending.values().any(|p| matches!(p, Pending::Mode(_))) {
            return Err("Mode change is pending.".into());
        }
        Ok(vec![self.request(
            "session/set_mode",
            json!({"sessionId":session,"modeId":id}),
            Pending::Mode(id.into()),
        )])
    }
    /// UI/runtime decisions use the stable transcript item, so a peer reusing a
    /// JSON-RPC ID cannot make an old decision answer a newer request.
    pub fn respond_permission(
        &mut self,
        item: ItemId,
        option: Option<&str>,
    ) -> Result<Value, String> {
        let id = self
            .permissions
            .values()
            .find(|p| p.item == item)
            .map(|p| p.id.clone())
            .ok_or("Permission is already resolved or belongs to another run.")?;
        self.respond(&id, option)
    }

    /// Selects the exact offered ID. `allow_always` is not relabelled as chat scope.
    pub fn respond(&mut self, id: &RequestId, option: Option<&str>) -> Result<Value, String> {
        let request = self
            .permissions
            .get(id)
            .ok_or("Permission is already resolved or belongs to another run.")?;
        let selected = option
            .map(|id| {
                request
                    .options
                    .iter()
                    .find(|o| o.option_id == id)
                    .ok_or("Permission option is not offered.")
            })
            .transpose()?;
        if selected.is_some_and(|s| {
            !matches!(
                s.kind.as_str(),
                "allow_once" | "allow_always" | "reject_once" | "reject_always"
            )
        }) {
            return Err("This ACP permission kind is not supported.".into());
        }
        let decision = match selected.map(|s| s.kind.as_str()) {
            Some("allow_once" | "allow_always") => Decision::Allowed,
            Some(_) => Decision::Denied,
            None => Decision::Cancelled,
        };
        if let Some(ix) = self.transcript.position(request.item) {
            self.transcript.update(ix, |body| {
                if let Body::Permission(p) = body {
                    p.decision = decision;
                }
            });
        }
        self.permissions.remove(id);
        if self.permissions.is_empty()
            && self.prompt.is_some()
            && self.status != Status::Interrupting
        {
            self.status = Status::Working;
        }
        Ok(wire::reply(
            id,
            match option {
                Some(option) => json!({"outcome":{"outcome":"selected","optionId":option}}),
                None => json!({"outcome":{"outcome":"cancelled"}}),
            },
        ))
    }
    pub fn complete_host_request(
        &mut self,
        id: &RequestId,
        result: Result<Value, String>,
    ) -> Result<Value, String> {
        if self.host_pending.remove(id).is_none() {
            return Err("Host request is already resolved or belongs to another run.".into());
        }
        Ok(match result {
            Ok(value) => wire::reply(id, value),
            Err(message) => wire::error(id, -32603, &message),
        })
    }
    pub fn interrupt(&mut self) -> Vec<Value> {
        let Some(session) = self.session_id.clone().filter(|_| self.prompt.is_some()) else {
            return vec![];
        };
        let mut lines = vec![];
        for id in self.permissions.keys().cloned().collect::<Vec<_>>() {
            if let Ok(line) = self.respond(&id, None) {
                lines.push(line);
            }
        }
        self.status = Status::Interrupting;
        lines.push(wire::notify("session/cancel", json!({"sessionId":session})));
        lines
    }
    pub fn on_event(&mut self, event: Event<Message>) -> Vec<Value> {
        match event {
            Event::Message(message) => self.on_message(message),
            Event::Diagnostic(message) => {
                if self.status == Status::Starting {
                    self.fail("Invalid ACP protocol during startup.");
                } else {
                    self.diagnostic(&message);
                }
                vec![]
            }
            Event::Exited(exit) => {
                self.expire();
                self.status = Status::Exited {
                    code: exit.code,
                    signal: exit.signal,
                };
                vec![]
            }
        }
    }
    pub fn on_message(&mut self, message: Message) -> Vec<Value> {
        if self.status.ended() {
            return vec![];
        }
        match message {
            Message::Response { id, result } => self.response(id, result),
            Message::Notification { method, params } => {
                if method == "session/update" {
                    self.update(&params);
                } else {
                    self.diagnostic("Unknown ACP notification ignored.");
                }
                vec![]
            }
            Message::Request { id, method, params } => self.agent_request(id, &method, params),
        }
    }
    fn response(&mut self, id: RequestId, result: Result<Value, Value>) -> Vec<Value> {
        let Some(pending) = self.pending.remove(&id) else {
            self.diagnostic("Unmatched ACP response ignored.");
            return vec![];
        };
        let result = match result {
            Ok(value) => value,
            Err(_) => {
                // Protocol error data may contain credentials or prompt fragments.
                match pending {
                    Pending::Initialize | Pending::Open => {
                        self.fail("ACP initialization or session open failed.")
                    }
                    Pending::Prompt => {
                        // Refused before any turn event: the agent didn't take it.
                        self.receipt(Receipt::Refused);
                        self.finish(Outcome::Failed("ACP prompt failed.".into()))
                    }
                    _ => self.diagnostic(
                        "ACP configuration change was refused; previous selection retained.",
                    ),
                }
                return vec![];
            }
        };
        match pending {
            Pending::Initialize => {
                if result["protocolVersion"].as_u64() != Some(1) {
                    self.fail("ACP protocol version is not supported (expected v1).");
                    return vec![];
                }
                let caps = &result["agentCapabilities"];
                self.capabilities = Capabilities {
                    resume: caps["loadSession"] == true,
                    images: caps["promptCapabilities"]["image"] == true,
                    embedded_context: caps["promptCapabilities"]["embeddedContext"] == true,
                    cancel: true,
                    ..Default::default()
                };
                let mut params = json!({"cwd":self.start.cwd,"mcpServers":[]});
                let method = if let Some(id) = &self.start.resume {
                    if !self.capabilities.resume {
                        self.fail("ACP agent does not support loading this session.");
                        return vec![];
                    }
                    params["sessionId"] = json!(id);
                    self.session_id = Some(id.clone());
                    if !self.start.has_checkpoint {
                        self.replay = Some(Transcript::default());
                    }
                    "session/load"
                } else {
                    "session/new"
                };
                vec![self.request(method, params, Pending::Open)]
            }
            Pending::Open => {
                if self.start.resume.is_none() {
                    let Some(id) = result["sessionId"].as_str().filter(|id| !id.is_empty()) else {
                        self.fail("ACP session response has no ID.");
                        return vec![];
                    };
                    self.session_id = Some(id.into());
                } else if let Some(id) = result["sessionId"].as_str()
                    && Some(id) != self.session_id.as_deref()
                {
                    self.fail("ACP loaded a different session.");
                    return vec![];
                }
                if let Some(replay) = self.replay.take() {
                    self.transcript = replay;
                }
                self.text = None;
                self.tools.clear();
                self.read_configuration(&result);
                self.status = Status::Ready;
                vec![]
            }
            Pending::Prompt => {
                self.receipt(Receipt::Delivered);
                let outcome = match result["stopReason"].as_str() {
                    Some("end_turn") => Outcome::Completed,
                    Some("cancelled") => Outcome::Interrupted,
                    Some("max_tokens" | "max_turn_requests") => {
                        Outcome::Limit(result["stopReason"].as_str().unwrap().into())
                    }
                    Some("refusal") => Outcome::Failed("The agent refused the prompt.".into()),
                    _ => {
                        Outcome::Failed("ACP prompt ended without a recognized stop reason.".into())
                    }
                };
                let replies = self
                    .permissions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .filter_map(|id| self.respond(&id, None).ok())
                    .collect();
                self.finish(outcome);
                replies
            }
            Pending::Config => {
                self.read_configuration(&result);
                vec![]
            }
            Pending::Mode(value) => {
                self.current_mode = Some(value);
                vec![]
            }
            Pending::Model(value) => {
                self.current_model = Some(value);
                vec![]
            }
        }
    }
    fn agent_request(&mut self, id: RequestId, method: &str, params: Value) -> Vec<Value> {
        if params["sessionId"].as_str() != self.session_id.as_deref() || self.session_id.is_none() {
            return vec![wire::error(&id, -32602, "Unknown session.")];
        }
        if self.prompt.is_some() && self.replay.is_none() {
            self.receipt(Receipt::Delivered);
        }
        if self.permissions.contains_key(&id) || self.host_pending.contains_key(&id) {
            return vec![wire::error(
                &id,
                -32600,
                "Duplicate outstanding request ID.",
            )];
        }
        if self.permissions.len() + self.host_pending.len() >= MAX_PENDING {
            return vec![wire::error(&id, -32000, "Too many pending requests.")];
        }
        if method == "session/request_permission" {
            if self.prompt.is_none() || self.status == Status::Interrupting {
                return vec![wire::reply(&id, json!({"outcome":{"outcome":"cancelled"}}))];
            }
            let Ok(options) =
                serde_json::from_value::<Vec<PermissionOption>>(params["options"].clone())
            else {
                return vec![wire::error(&id, -32602, "Invalid permission options.")];
            };
            let call = &params["toolCall"];
            let ix = self.transcript.push(
                None,
                Body::Permission(Permission {
                    request_id: serde_json::to_string(&id).unwrap_or_default(),
                    call_id: call["toolCallId"].as_str().map(str::to_owned),
                    tool: call["title"].as_str().unwrap_or("Tool").into(),
                    input: call["rawInput"].clone(),
                    title: call["title"].as_str().map(str::to_owned),
                    description: None,
                    kind: PermissionKind::Tool,
                    decision: Decision::Pending,
                    remember: None,
                    resolution: None,
                }),
            );
            self.permissions.insert(
                id.clone(),
                PermissionRequest {
                    id,
                    item: self.transcript.get(ix).unwrap().id,
                    options,
                },
            );
            self.status = Status::NeedsDecision;
            return vec![];
        }
        let enabled = match method {
            "fs/read_text_file" => self.start.host.read_text_file,
            "fs/write_text_file" => self.start.host.write_text_file,
            "terminal/create"
            | "terminal/output"
            | "terminal/wait_for_exit"
            | "terminal/kill"
            | "terminal/release" => self.start.host.terminal,
            _ => false,
        };
        if !enabled {
            return vec![wire::error(&id, -32601, "Host service is not available.")];
        }
        if self.pending.values().any(|p| matches!(p, Pending::Open)) {
            return vec![wire::error(
                &id,
                -32600,
                "Host effects are unavailable during session replay.",
            )];
        }
        match serde_json::from_value::<HostOperation>(json!({"method":method,"params":params})) {
            Ok(operation) => {
                self.host_pending.insert(id.clone(), ());
                self.host_requests.push_back(HostRequest {
                    id,
                    session_id: self.session_id.clone().unwrap(),
                    operation,
                });
                vec![]
            }
            Err(_) => vec![wire::error(&id, -32602, "Invalid host service parameters.")],
        }
    }
    fn update(&mut self, params: &Value) {
        if params["sessionId"].as_str() != self.session_id.as_deref() || self.session_id.is_none() {
            self.diagnostic("Update for an unknown ACP session ignored.");
            return;
        }
        if self.prompt.is_some() && self.replay.is_none() {
            // The first update of the prompt's turn is evidence it was taken.
            self.receipt(Receipt::Delivered);
        }
        let update = &params["update"];
        match update["sessionUpdate"].as_str() {
            Some("config_option_update") => {
                self.read_configuration(update);
                return;
            }
            Some("current_mode_update") => {
                self.current_mode = update["currentModeId"].as_str().map(str::to_owned);
                return;
            }
            Some("available_commands_update") => {
                self.commands_received = true;
                self.commands = update["availableCommands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| {
                        Some(CommandInfo {
                            name: c["name"].as_str()?.into(),
                            description: c["description"].as_str().unwrap_or_default().into(),
                            hint: c["input"]["hint"].as_str().map(str::to_owned),
                            ..Default::default()
                        })
                    })
                    .collect();
                return;
            }
            _ => {}
        }
        let loading = self.pending.values().any(|p| matches!(p, Pending::Open));
        if loading && self.start.has_checkpoint {
            return;
        }
        if loading {
            self.replay_bytes = self.replay_bytes.saturating_add(update.to_string().len());
            if self.replay_bytes > MAX_REPLAY_BYTES {
                self.fail("ACP replay exceeds the import limit.");
                return;
            }
        } else if self.prompt.is_none() {
            self.diagnostic("ACP transcript update outside a turn ignored.");
            return;
        }
        let transcript = if loading {
            let Some(replay) = self.replay.as_mut() else {
                return;
            };
            replay
        } else {
            &mut self.transcript
        };
        if update["sessionUpdate"] != "user_message_chunk" {
            self.replay_user = None;
        }
        match update["sessionUpdate"].as_str() {
            Some("agent_message_chunk" | "agent_thought_chunk" | "user_message_chunk") => {
                let Some(text) = update["content"]["text"].as_str() else {
                    return;
                };
                let kind = update["sessionUpdate"].as_str().unwrap();
                if kind == "user_message_chunk" {
                    if loading {
                        if let Some(ix) = self.replay_user {
                            transcript.update(ix, |body| {
                                if let Body::User(message) = body {
                                    message.text.push_str(text);
                                }
                            });
                            return;
                        }
                        let turn = transcript.allocate_turn();
                        transcript.set_current_turn(turn);
                        let ix = transcript.push(
                            None,
                            Body::User(UserMessage {
                                text: text.into(),
                                delivery: Delivery::Sent,
                                uuid: None,
                                attachments: vec![],
                                context: None,
                                goal: false,
                            }),
                        );
                        self.replay_user = Some(ix);
                        self.text = None;
                    }
                    return;
                }
                let thinking = kind == "agent_thought_chunk";
                if let Some((ix, previous)) = self.text
                    && previous == thinking
                {
                    transcript.update(ix, |body| match body {
                        Body::Text(block) | Body::Thinking(block) => block.text.push_str(text),
                        _ => {}
                    });
                } else {
                    let block = TextBlock {
                        text: text.into(),
                        streaming: !loading,
                    };
                    let ix = transcript.push(
                        None,
                        if thinking {
                            Body::Thinking(block)
                        } else {
                            Body::Text(block)
                        },
                    );
                    self.text = Some((ix, thinking));
                }
            }
            Some("tool_call" | "tool_call_update") => {
                self.text = None;
                let Some(id) = update["toolCallId"].as_str() else {
                    return;
                };
                let ix = if let Some(ix) = self.tools.get(id) {
                    *ix
                } else {
                    let ix = transcript.push(
                        None,
                        Body::Tool(ToolCall {
                            kind: Some(super::backend::ToolKind::Other),
                            call_id: id.into(),
                            name: update["title"].as_str().unwrap_or("Tool").into(),
                            input: json!({}),
                            input_partial: String::new(),
                            state: ToolState::Running,
                            output: None,
                            elapsed_seconds: None,
                            activity: None,
                            background: false,
                        }),
                    );
                    self.tools.insert(id.into(), ix);
                    ix
                };
                transcript.update(ix, |body| {
                    if let Body::Tool(call) = body {
                        if let Some(title) = update["title"].as_str() {
                            call.name = title.into();
                        }
                        if let Some(kind) = update["kind"].as_str() {
                            call.kind = Some(acp_tool_kind(kind));
                        }
                        let locations = update
                            .get("locations")
                            .cloned()
                            .or_else(|| call.input.get("_acp_locations").cloned());
                        if let Some(input) = update.get("rawInput") {
                            call.input = input.clone();
                        }
                        // Keep locations across partial raw-input updates.
                        if let Some(locations) = locations {
                            if !call.input.is_object() {
                                call.input = json!({"rawInput":call.input});
                            }
                            call.input["_acp_locations"] = locations;
                        }
                        if let Some(status) = update["status"].as_str() {
                            call.state = match status {
                                "completed" => ToolState::Succeeded,
                                "failed" => ToolState::Failed,
                                "pending" | "in_progress" => ToolState::Running,
                                _ => call.state,
                            };
                        }
                        if let Some(content) = update.get("content") {
                            call.output = Some(ToolOutput {
                                text: content
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|c| c["content"]["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                                is_error: call.state == ToolState::Failed,
                                structured: Some(content.clone()),
                            });
                        }
                    }
                });
            }
            Some("plan") => {
                if let Ok(plan) =
                    serde_json::from_value::<Vec<PlanEntry>>(update["entries"].clone())
                {
                    let input = json!({"todos":plan});
                    if let Some(ix) = self.plan_item {
                        transcript.update(ix, |body| {
                            if let Body::Tool(tool) = body {
                                tool.input = input;
                            }
                        });
                    } else {
                        self.plan_item = Some(transcript.push(
                            None,
                            Body::Tool(ToolCall {
                                kind: Some(super::backend::ToolKind::Plan),
                                call_id: "acp-plan".into(),
                                name: "plan".into(),
                                input,
                                input_partial: String::new(),
                                state: ToolState::Succeeded,
                                output: None,
                                elapsed_seconds: None,
                                activity: None,
                                background: false,
                            }),
                        ));
                    }
                    self.plan = plan;
                }
            }
            _ => self.diagnostic("Unknown ACP session update ignored."),
        }
    }
    fn read_configuration(&mut self, value: &Value) {
        if let Some(options) = value["configOptions"].as_array() {
            self.options = options
                .iter()
                .filter_map(|o| {
                    if o["type"] != "select" {
                        return None;
                    }
                    let mut choices = vec![];
                    for choice in o["options"].as_array().into_iter().flatten() {
                        if let Some(group) = choice["options"].as_array() {
                            choices.extend(group.iter().filter_map(config_choice));
                        } else if let Some(choice) = config_choice(choice) {
                            choices.push(choice);
                        }
                    }
                    Some(SessionOption {
                        id: o["id"].as_str()?.into(),
                        label: o["name"].as_str()?.into(),
                        description: o["description"].as_str().map(str::to_owned),
                        category: o["category"].as_str().map(str::to_owned),
                        value: o["currentValue"].as_str()?.into(),
                        choices,
                        effect: OptionEffect::Immediate,
                    })
                })
                .collect();
        }
        if let Some(modes) = value["modes"]["availableModes"].as_array() {
            self.modes = modes.iter().filter_map(|m| named_choice(m, "id")).collect();
            self.current_mode = value["modes"]["currentModeId"].as_str().map(str::to_owned);
        }
        if let Some(models) = value["models"]["availableModels"].as_array() {
            self.models = models
                .iter()
                .filter_map(|m| named_choice(m, "modelId"))
                .collect();
            self.current_model = value["models"]["currentModelId"]
                .as_str()
                .map(str::to_owned);
        }
    }
    fn finish(&mut self, outcome: Outcome) {
        self.prompt = None;
        for ix in self.turn_start..self.transcript.items().len() {
            self.transcript.update(ix, |body| match body {
                Body::Text(b) | Body::Thinking(b) => b.streaming = false,
                Body::Tool(tool) if !tool.state.finished() => {
                    tool.state = if outcome == Outcome::Interrupted {
                        ToolState::Cancelled
                    } else {
                        ToolState::Failed
                    };
                }
                Body::Permission(p) if p.decision == Decision::Pending => {
                    p.decision = Decision::Cancelled
                }
                _ => {}
            });
        }
        self.permissions.clear();
        self.transcript.push(
            None,
            Body::TurnEnd(TurnEnd {
                outcome,
                usage: None,
                cost_usd: None,
                duration_ms: None,
                resume_at: None,
                quiet: false,
            }),
        );
        self.text = None;
        self.status = Status::Ready;
    }
    fn expire(&mut self) {
        self.receipt(Receipt::Unknown);
        self.replay = None;
        self.plan.clear();
        self.pending.clear();
        self.host_pending.clear();
        self.host_requests.clear();
        if self.prompt.is_some() {
            self.finish(Outcome::Failed(
                "Agent process ended; delivery may be unknown.".into(),
            ));
        }
        self.permissions.clear();
    }
    fn fail(&mut self, message: &str) {
        self.expire();
        self.status = Status::Failed(message.into());
    }
    fn diagnostic(&mut self, message: &str) {
        if self.diagnostics.len() == 50 {
            self.diagnostics.pop_front();
        }
        self.diagnostics
            .push_back(message.chars().take(512).collect());
    }
}
fn config_choice(v: &Value) -> Option<OptionChoice> {
    Some(OptionChoice {
        id: v["value"].as_str()?.into(),
        label: v["name"].as_str()?.into(),
        description: v["description"].as_str().map(str::to_owned),
    })
}
fn named_choice(v: &Value, key: &str) -> Option<OptionChoice> {
    Some(OptionChoice {
        id: v[key].as_str()?.into(),
        label: v["name"].as_str()?.into(),
        description: v["description"].as_str().map(str::to_owned),
    })
}

fn acp_tool_kind(kind: &str) -> super::backend::ToolKind {
    use super::backend::ToolKind;
    match kind {
        "read" => ToolKind::Read,
        "edit" | "delete" | "move" => ToolKind::Edit,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Shell,
        "fetch" => ToolKind::Web,
        "think" => ToolKind::Plan,
        _ => ToolKind::Other,
    }
}
