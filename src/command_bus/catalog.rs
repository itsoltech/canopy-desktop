use super::*;

/// Catalog effects use the existing app owners after shared bus admission.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CatalogCommand {
    Create {
        agent: String,
        profile: String,
    },
    Close {
        plan: String,
    },
    /// Local completion of chat.delete for retained closed history and scratch
    /// data. The typed action remains in the fingerprint; it has no public RPC.
    ForgetClosed {
        chat: String,
    },
    OpenProject {
        path: String,
    },
    CloseProject {
        plan: String,
    },
    CloseTerminal {
        terminal: String,
        plan: String,
    },
}
pub(super) static REGISTRY: &[CommandMetadata] = &[
    CommandMetadata {
        name: "chat.create",
        risk: Risk::Delivery,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "chat.delete",
        risk: Risk::Delivery,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "projects.open",
        risk: Risk::Configuration,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "projects.close",
        risk: Risk::Delivery,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "terminal.close",
        risk: Risk::Delivery,
        scope: host::TERMINAL,
        workspace_attachment_scope: 0,
    },
];
impl CatalogCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Create { .. } => 0,
            Self::Close { .. } | Self::ForgetClosed { .. } => 1,
            Self::OpenProject { .. } => 2,
            Self::CloseProject { .. } => 3,
            Self::CloseTerminal { .. } => 4,
        }]
    }
    pub(super) fn normalize(&self) -> Result<()> {
        let valid = match self {
            Self::Create { agent, profile } => [agent, profile]
                .iter()
                .all(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)),
            Self::Close { plan } | Self::CloseProject { plan } => host::valid_id(plan),
            Self::ForgetClosed { chat } => host::valid_id(chat),
            Self::CloseTerminal { terminal, plan } => {
                host::valid_id(terminal) && host::valid_id(plan)
            }
            Self::OpenProject { path } => {
                !path.is_empty()
                    && path.len() <= 4096
                    && !path.contains('\0')
                    && std::path::Path::new(path).is_absolute()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Error::InvalidRuntimeState)
        }
    }
}
