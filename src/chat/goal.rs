//! Goals (`/goal`): a condition the agent keeps working toward over several turns until
//! it holds. Claude Code and Codex run the loop themselves; this is the provider-neutral
//! state and the parsing of what each reports. Pure: no I/O.
use serde::{Deserialize, Serialize};

/// The command that sets a goal, typed as `/goal`.
pub const COMMAND: &str = "goal";
/// The longest condition either CLI takes.
pub const LIMIT: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    /// The agent works toward it.
    Pursuing,
    /// Set, but the agent is idle (Claude after Stop or a restart): it continues with
    /// the next message.
    Set,
    /// Codex: paused by Stop, Pause or a restart; Resume continues it.
    Paused,
    /// Codex stopped short: it needs the user.
    Blocked,
    UsageLimited,
    BudgetLimited,
}
impl GoalStatus {
    /// Codex can continue it on its own after Resume.
    pub fn resumable(self) -> bool {
        matches!(
            self,
            Self::Paused | Self::Blocked | Self::UsageLimited | Self::BudgetLimited
        )
    }
}

/// Unix seconds now.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The goal of a chat and what it has cost so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub objective: String,
    pub status: GoalStatus,
    /// Unix seconds when it was set (Claude reports no time of its own).
    pub since: u64,
    /// Codex accounting.
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub budget: Option<u64>,
    #[serde(default)]
    pub seconds: Option<u64>,
    /// Turns the agent started for it on its own (Codex).
    #[serde(default)]
    pub turns: u32,
    /// Claude: checks that found the condition not yet met, and the latest reason.
    #[serde(default)]
    pub checks: u32,
    #[serde(default)]
    pub last_check: Option<String>,
}
impl Goal {
    pub fn new(objective: String, status: GoalStatus, since: u64) -> Self {
        Self {
            objective,
            status,
            since,
            tokens: None,
            budget: None,
            seconds: None,
            turns: 0,
            checks: 0,
            last_check: None,
        }
    }
    /// How long it has run: the agent's own count when it keeps one.
    pub fn elapsed(&self, now: u64) -> u64 {
        self.seconds
            .unwrap_or_else(|| now.saturating_sub(self.since))
    }
    /// The ending card for this goal.
    pub fn end(&self, reason: EndReason, note: Option<String>, now: u64) -> GoalEnd {
        GoalEnd {
            objective: self.objective.clone(),
            reason,
            note,
            turns: self.turns,
            checks: self.checks,
            seconds: Some(self.elapsed(now)),
            tokens: self.tokens,
        }
    }
}

/// Why a goal ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    Achieved,
    /// The agent judged the condition impossible to meet.
    Impossible,
    ClearedByUser,
    ClearedOnSwitch,
    ClearedOnEdit,
    /// The agent hit an error it can't get past (auth, credits, context).
    ClearedByError,
}

/// A finished goal, as the conversation shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoalEnd {
    pub objective: String,
    pub reason: EndReason,
    /// The agent's reason or the error, when it gave one.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub turns: u32,
    #[serde(default)]
    pub checks: u32,
    #[serde(default)]
    pub seconds: Option<u64>,
    #[serde(default)]
    pub tokens: Option<u64>,
}

/// What an agent reported about its goal.
#[derive(Clone, Debug, PartialEq)]
pub enum GoalEvent {
    /// Claude: the goal is set (`Goal set: …`).
    Set(String),
    /// Claude: the status asked with `/goal` while one is active.
    Active {
        objective: String,
        last_check: Option<String>,
    },
    /// Claude: feedback a Stop hook gave the agent (raw); a check of the goal when it
    /// names its condition (`parse_feedback`).
    Feedback(String),
    /// Claude: `/goal clear` removed it (`Some`) or there was none (`None`).
    Cleared(Option<String>),
    /// Claude cleared it after an error the user has to fix.
    ClearedByError(String),
    /// Claude: something about the goal worth a line (paused or retrying after errors).
    Note(String),
    /// Claude: the turn pursuing it ended; `completed` when it ran to its end on its own.
    TurnEnded { completed: bool },
    /// Codex: its goal as it is now.
    Codex(CodexGoal),
    /// Codex removed its goal.
    CodexCleared,
}

/// Codex's goal (`thread/goal/updated`).
#[derive(Clone, Debug, PartialEq)]
pub struct CodexGoal {
    pub objective: String,
    /// As Codex names it: active, paused, blocked, usageLimited, budgetLimited, complete.
    pub status: String,
    pub tokens: u64,
    pub seconds: u64,
    pub budget: Option<u64>,
}
impl CodexGoal {
    /// `None` once it's complete.
    pub fn status(&self) -> Option<GoalStatus> {
        Some(match self.status.as_str() {
            "complete" => return None,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "usageLimited" => GoalStatus::UsageLimited,
            "budgetLimited" => GoalStatus::BudgetLimited,
            _ => GoalStatus::Pursuing,
        })
    }
}

const SET: &str = "Goal set: ";
const ACTIVE: &str = "Goal active: ";
const CLEARED: &str = "Goal cleared: ";
const CLEARED_BY_ERROR: &str = "Goal cleared after an unrecoverable error";
const LAST_CHECK: &str = "\nLast check: ";
const FEEDBACK: &str = "Stop hook feedback:\n[";

/// Reads Claude's own reply to `/goal` (a synthetic assistant message). Text that isn't
/// about a goal is `None` and stays an ordinary reply.
pub fn parse_reply(text: &str) -> Option<GoalEvent> {
    let text = text.trim();
    if let Some(objective) = text.strip_prefix(SET) {
        let objective = objective.trim();
        return (!objective.is_empty()).then(|| GoalEvent::Set(objective.to_owned()));
    }
    if text.starts_with(CLEARED_BY_ERROR) {
        return Some(GoalEvent::ClearedByError(text.to_owned()));
    }
    if let Some(objective) = text.strip_prefix(CLEARED) {
        return Some(GoalEvent::Cleared(Some(objective.trim().to_owned())));
    }
    if text.starts_with("No goal set") {
        return Some(GoalEvent::Cleared(None));
    }
    if let Some(rest) = text.strip_prefix(ACTIVE) {
        let (head, last_check) = match rest.rfind(LAST_CHECK) {
            Some(at) => (&rest[..at], Some(rest[at + LAST_CHECK.len()..].trim())),
            None => (rest, None),
        };
        // `<condition> (2 turns)` or `<condition> (not yet evaluated)`.
        let objective = head
            .trim_end()
            .strip_suffix(')')
            .and_then(|head| head.rfind(" (").map(|at| &head[..at]))
            .unwrap_or(head)
            .trim();
        return Some(GoalEvent::Active {
            objective: objective.to_owned(),
            last_check: last_check.filter(|c| !c.is_empty()).map(str::to_owned),
        });
    }
    if text.starts_with("Goal paused") || text.starts_with("Goal still active") {
        return Some(GoalEvent::Note(text.to_owned()));
    }
    None
}

/// The reason in a check that found `objective` not yet met (a synthetic user message
/// `Stop hook feedback:\n[<objective>]: <reason>`). Feedback of other Stop hooks is `None`.
pub fn parse_feedback(text: &str, objective: &str) -> Option<String> {
    let rest = text.strip_prefix(FEEDBACK)?;
    let reason = rest.strip_prefix(objective)?.strip_prefix("]: ")?;
    Some(reason.trim().to_owned())
}

/// What a goal's row says about it.
pub fn title(goal: &Goal, working: bool) -> &'static str {
    match goal.status {
        GoalStatus::Pursuing if working => "Pursuing goal",
        GoalStatus::Pursuing | GoalStatus::Set => "Goal set",
        GoalStatus::Paused => "Goal paused",
        GoalStatus::Blocked => "Goal blocked",
        GoalStatus::UsageLimited => "Usage limit reached",
        GoalStatus::BudgetLimited => "Token budget reached",
    }
}

/// `3 turns · 42k / 200k tokens · 6m` (Codex) or `2 checks · 11m` (Claude); `None`
/// before anything is counted.
pub fn usage(goal: &Goal, now: u64) -> Option<String> {
    let mut parts = vec![];
    if goal.turns > 0 {
        parts.push(plural(goal.turns, "turn"));
    }
    if goal.checks > 0 {
        parts.push(plural(goal.checks, "check"));
    }
    match (goal.tokens.filter(|t| *t > 0), goal.budget) {
        (Some(tokens), Some(budget)) => parts.push(format!(
            "{} / {} tokens",
            tokens_text(tokens),
            tokens_text(budget)
        )),
        (Some(tokens), None) => parts.push(format!("{} tokens", tokens_text(tokens))),
        _ => {}
    }
    let seconds = goal.elapsed(now);
    if seconds >= 60 {
        parts.push(duration_text(seconds));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The ending card's numbers: `3 turns · 2 checks · 14m · 61k tokens`.
pub fn summary(end: &GoalEnd) -> String {
    let mut parts = vec![];
    if end.turns > 0 {
        parts.push(plural(end.turns, "turn"));
    }
    if end.checks > 0 {
        parts.push(plural(end.checks, "check"));
    }
    if let Some(seconds) = end.seconds {
        parts.push(duration_text(seconds));
    }
    if let Some(tokens) = end.tokens.filter(|t| *t > 0) {
        parts.push(format!("{} tokens", tokens_text(tokens)));
    }
    parts.join(" · ")
}

pub fn end_title(reason: EndReason) -> &'static str {
    match reason {
        EndReason::Achieved => "Goal achieved",
        EndReason::Impossible => "Goal cleared: judged impossible",
        EndReason::ClearedByUser => "Goal cleared",
        EndReason::ClearedOnSwitch => "Goal cleared · agent switched",
        EndReason::ClearedOnEdit => "Goal cleared · message edited",
        EndReason::ClearedByError => "Goal cleared after an error",
    }
}

fn plural(count: u32, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

fn tokens_text(tokens: u64) -> String {
    match tokens {
        0..1_000 => tokens.to_string(),
        1_000..1_000_000 => format!("{}k", (tokens + 500) / 1_000),
        _ => {
            let millions = format!("{:.1}", tokens as f64 / 1_000_000.0);
            format!("{}m", millions.trim_end_matches(".0"))
        }
    }
}

fn duration_text(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONDITION: &str = "counter.txt contains 4 (one step per turn)";

    #[test]
    fn claude_replies_about_its_goal_are_recognised() {
        assert_eq!(
            parse_reply(&format!("Goal set: {CONDITION}")),
            Some(GoalEvent::Set(CONDITION.into()))
        );
        assert_eq!(
            parse_reply(&format!("Goal cleared: {CONDITION}")),
            Some(GoalEvent::Cleared(Some(CONDITION.into())))
        );
        assert_eq!(
            parse_reply("No goal set. Usage: `/goal <condition>`"),
            Some(GoalEvent::Cleared(None))
        );
        assert_eq!(parse_reply("Compacted the conversation."), None);
    }

    #[test]
    fn the_status_keeps_brackets_inside_the_condition() {
        let reply = format!(
            "Goal active: {CONDITION} (1 turn)\nLast check: counter.txt holds 1 (from\n`cat`), not 4."
        );
        assert_eq!(
            parse_reply(&reply),
            Some(GoalEvent::Active {
                objective: CONDITION.into(),
                last_check: Some("counter.txt holds 1 (from\n`cat`), not 4.".into()),
            })
        );
        assert_eq!(
            parse_reply(&format!("Goal active: {CONDITION} (not yet evaluated)")),
            Some(GoalEvent::Active {
                objective: CONDITION.into(),
                last_check: None,
            })
        );
    }

    #[test]
    fn only_checks_of_this_goal_are_its_checks() {
        let feedback = format!("Stop hook feedback:\n[{CONDITION}]: It holds 2, not 4.");
        assert_eq!(
            parse_feedback(&feedback, CONDITION),
            Some("It holds 2, not 4.".into())
        );
        assert_eq!(
            parse_feedback("Stop hook feedback:\n[npm run lint]: 3 errors", CONDITION),
            None
        );
    }

    #[test]
    fn usage_reads_like_the_mockup() {
        let mut goal = Goal::new("x".into(), GoalStatus::Pursuing, 1_000);
        assert_eq!(usage(&goal, 1_030), None);
        goal.turns = 3;
        goal.tokens = Some(42_300);
        goal.budget = Some(200_000);
        goal.seconds = Some(365);
        assert_eq!(
            usage(&goal, 0).as_deref(),
            Some("3 turns · 42k / 200k tokens · 6m")
        );
        assert_eq!(tokens_text(1_240_000), "1.2m");
        assert_eq!(tokens_text(2_000_000), "2m");
    }
}
