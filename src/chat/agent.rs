//! Agent identity and backend selection are data, independently of legacy pane providers.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendKind {
    Claude,
    Codex,
    Acp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub backend: BackendKind,
    pub command: PathBuf,
    pub arguments: Vec<String>,
    /// Profile references, never credentials. The launcher resolves secrets on the host.
    pub profile: Option<String>,
    pub environment: HashMap<String, String>,
}
impl AgentDefinition {
    pub fn acp(id: String, name: String, command: PathBuf) -> Self {
        Self {
            id,
            name,
            backend: BackendKind::Acp,
            command,
            arguments: vec![],
            profile: None,
            environment: HashMap::new(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.name.is_empty() || self.command.as_os_str().is_empty() {
            return Err("Agent identity and command are required.".into());
        }
        if self.arguments.iter().any(|arg| arg.contains('\0'))
            || self.environment.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['=', '\0']) || value.contains('\0')
            })
        {
            return Err("Invalid agent arguments or environment.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Capabilities {
    pub resume: bool,
    pub images: bool,
    pub embedded_context: bool,
    pub cancel: bool,
    pub steer: bool,
    pub fork: bool,
    pub rewind: bool,
    pub goal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionEffect {
    Immediate,
    NextTurn,
    Restart,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OptionChoice {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}
impl OptionChoice {
    pub fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            description: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub value: String,
    pub choices: Vec<OptionChoice>,
    pub effect: OptionEffect,
}
