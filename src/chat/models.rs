//! The model picker's lists (DEC-097…099): every account's models, remembered so the
//! picker opens with them at once, fetched again in the background from the account's
//! CLI (Claude `initialize`, Codex `model/list`), and the models marked with a heart.
use super::config::ModelInfo;
use super::launch::Launch;
use super::process::{Event, Process};
use super::provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// How long the CLI gets to list its models.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_LINE: usize = 8 * 1024 * 1024;

/// A model marked with a heart: `model` `None` is the account's default entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub profile: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// What the picker remembers between runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    /// Each account's (profile id's) models as last listed.
    pub models: BTreeMap<String, Vec<ModelInfo>>,
    pub favorites: Vec<Favorite>,
}

impl Catalog {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Remembers `profile`'s models; `true` when they differ from what was kept.
    pub fn set_models(&mut self, profile: &str, models: Vec<ModelInfo>) -> bool {
        if models.is_empty() || self.models.get(profile) == Some(&models) {
            return false;
        }
        self.models.insert(profile.to_owned(), models);
        true
    }

    pub fn is_favorite(&self, profile: &str, model: Option<&str>) -> bool {
        self.favorites
            .iter()
            .any(|f| f.profile == profile && f.model.as_deref() == model)
    }

    /// Adds or removes the heart.
    pub fn toggle_favorite(&mut self, profile: &str, model: Option<&str>) {
        if self.is_favorite(profile, model) {
            self.favorites
                .retain(|f| !(f.profile == profile && f.model.as_deref() == model));
        } else {
            self.favorites.push(Favorite {
                profile: profile.to_owned(),
                model: model.map(str::to_owned),
            });
        }
    }
}

/// Whether a model matches the picker's search (its name or value).
pub fn matches(model: &ModelInfo, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || model.label().to_lowercase().contains(&query)
        || model.value.to_lowercase().contains(&query)
}

fn json_line(line: &[u8]) -> Result<Option<Value>, String> {
    Ok(serde_json::from_slice(line).ok())
}

/// Starts the account's CLI only to list its models, and closes it. Blocks: call off
/// the UI thread.
pub fn fetch(launch: Launch, provider: Provider) -> Result<Vec<ModelInfo>, String> {
    let (process, events) = Process::start(launch.spec, launch.files, json_line, MAX_LINE)?;
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let result = (|| {
        let init = "canopy-models-1";
        match provider {
            Provider::Claude => {
                process.send(&super::claude::wire::initialize(init))?;
                let response = wait(&events, deadline, |line| {
                    let response = line.get("response")?;
                    (line.get("type")?.as_str()? == "control_response"
                        && response.get("request_id")?.as_str()? == init)
                        .then(|| response.clone())
                })?;
                if response.get("subtype").and_then(Value::as_str) == Some("error") {
                    return Err(response
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("The agent couldn't list its models.")
                        .to_owned());
                }
                Ok(super::claude::wire::models(
                    response.get("response").unwrap_or(&Value::Null),
                ))
            }
            Provider::Codex => {
                use super::codex::wire;
                process.send(&wire::initialize(init))?;
                wait(&events, deadline, |line| reply(line, init))??;
                process.send(&wire::initialized())?;
                let list = "canopy-models-2";
                process.send(&wire::model_list(list))?;
                let result = wait(&events, deadline, |line| reply(line, list))??;
                Ok(wire::models(&result))
            }
        }
    })();
    let _ = process.close_with(Duration::from_millis(200), Duration::from_secs(1));
    result
}

/// The result of request `id`, or its error message.
fn reply(line: &Value, id: &str) -> Option<Result<Value, String>> {
    if line.get("id")?.as_str()? != id {
        return None;
    }
    Some(match line.get("error") {
        Some(error) => Err(error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The agent couldn't list its models.")
            .to_owned()),
        None => Ok(line.get("result").cloned().unwrap_or(Value::Null)),
    })
}

/// The first line `pick` takes, before `deadline`.
fn wait<T>(
    events: &async_channel::Receiver<Event<Value>>,
    deadline: Instant,
    mut pick: impl FnMut(&Value) -> Option<T>,
) -> Result<T, String> {
    loop {
        match events.try_recv() {
            Ok(Event::Message(line)) => {
                if let Some(found) = pick(&line) {
                    return Ok(found);
                }
            }
            Ok(Event::Exited(exit)) => {
                return Err(if exit.stderr.trim().is_empty() {
                    "The agent stopped before listing its models.".to_owned()
                } else {
                    exit.stderr.trim().to_owned()
                });
            }
            Ok(_) => {}
            Err(async_channel::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    return Err("The agent took too long to list its models.".to_owned());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(async_channel::TryRecvError::Closed) => {
                return Err("The agent stopped before listing its models.".to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(value: &str) -> ModelInfo {
        ModelInfo {
            value: value.into(),
            display_name: value.into(),
            ..Default::default()
        }
    }

    #[test]
    fn lists_are_remembered_and_hearts_toggle() {
        let mut catalog = Catalog::default();
        assert!(!catalog.set_models("claude-default", vec![]));
        assert!(catalog.set_models("claude-default", vec![model("opus")]));
        assert!(!catalog.set_models("claude-default", vec![model("opus")]));
        catalog.toggle_favorite("claude-default", Some("opus"));
        catalog.toggle_favorite("codex-default", None);
        assert!(catalog.is_favorite("claude-default", Some("opus")));
        assert!(catalog.is_favorite("codex-default", None));
        catalog.toggle_favorite("claude-default", Some("opus"));
        assert!(!catalog.is_favorite("claude-default", Some("opus")));
        let back = Catalog::from_json(&catalog.to_json().unwrap());
        assert_eq!(back, catalog);
        assert_eq!(Catalog::from_json("not json"), Catalog::default());
    }

    #[test]
    fn replies_and_search() {
        let ok = serde_json::json!({"id": "a", "result": {"data": []}});
        assert_eq!(reply(&ok, "a"), Some(Ok(serde_json::json!({"data": []}))));
        assert_eq!(reply(&ok, "b"), None);
        let error = serde_json::json!({"id": "a", "error": {"message": "not logged in"}});
        assert_eq!(reply(&error, "a"), Some(Err("not logged in".into())));
        let opus = ModelInfo {
            value: "claude-opus-5-5".into(),
            display_name: "Opus".into(),
            description: "Opus 5.5 · most capable".into(),
            ..Default::default()
        };
        assert!(matches(&opus, "opus") && matches(&opus, "5-5") && matches(&opus, ""));
        assert!(!matches(&opus, "gpt"));
    }
}
