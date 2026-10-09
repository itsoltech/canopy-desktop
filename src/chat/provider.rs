//! Which agent a chat talks to. The pane's tool id names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Provider {
    #[default]
    Claude,
    Codex,
}
impl Provider {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    pub fn from_tool(tool: &str) -> Option<Self> {
        match tool {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
    pub fn tool(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
    /// The product name in messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}
