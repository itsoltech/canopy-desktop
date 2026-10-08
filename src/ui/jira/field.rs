use crate::ui::{
    components::*,
    task_edit::composer::{ComposerChanged, MarkdownComposer},
    theme as t,
};
use canopy_desktop::integrations::jira::{JiraField, adf, choice_value, field_value};
use gpui_kit::{
    base::Disableable,
    component::{
        combobox::{ComboboxEvent, ComboboxState},
        input::{InputEvent, InputState, TextareaState},
        select::SearchableVec,
    },
    *,
};
use serde_json::Value;

type ChoiceState = ComboboxState<SearchableVec<SelectOption>>;

pub struct FieldChanged;
impl EventEmitter<FieldChanged> for FieldInput {}
enum Control {
    Boolean,
    Text(Entity<InputState>),
    Markdown(Entity<MarkdownComposer>),
    Structured(Entity<TextareaState>),
    Choices(Entity<ChoiceState>),
}
pub struct FieldInput {
    pub field: JiraField,
    baseline: Value,
    selected: Vec<Value>,
    choice_options: Vec<(String, String, Value)>,
    control: Control,
    initial: String,
    initial_draft: Value,
    enabled: bool,
    _events: Vec<Subscription>,
}
fn choice_name(v: &Value) -> String {
    if let (Some(key), Some(summary)) = (v["key"].as_str(), v["fields"]["summary"].as_str()) {
        return format!("{key} — {summary}");
    }
    if let Some(display_name) = v["displayName"].as_str() {
        return v["name"]
            .as_str()
            .filter(|login| *login != display_name)
            .map(|login| format!("{display_name} (@{login})"))
            .unwrap_or_else(|| display_name.to_owned());
    }
    v.as_str()
        .or(v["name"].as_str())
        .or(v["value"].as_str())
        .or(v["key"].as_str())
        .or(v["fullName"].as_str())
        .unwrap_or("Value")
        .into()
}
fn selected_choice_label(value: &Value) -> String {
    if let Some(child) = value.get("child") {
        return format!(
            "{} / {}",
            choice_name(value.get("value").unwrap_or(value)),
            choice_name(child.get("value").unwrap_or(child))
        );
    }
    choice_name(value)
}
fn choice_key(field: &JiraField, value: &Value) -> String {
    if field.schema["type"] == "user" || field.schema["items"] == "user" {
        return value["accountId"]
            .as_str()
            .or_else(|| value.as_str())
            .unwrap_or_default()
            .to_owned();
    }
    if let Some(child) = value.get("child") {
        let parent = value
            .get("value")
            .map(|parent| choice_key(field, parent))
            .unwrap_or_default();
        return format!(
            "{parent}::{}",
            choice_key(field, child.get("value").unwrap_or(child))
        );
    }
    value["key"]
        .as_str()
        .or(value["id"].as_str())
        .or(value["value"].as_str())
        .or(value["name"].as_str())
        .or(value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn cascading_value(parent: &Value, child: &Value) -> Value {
    let parent_value = parent
        .get("value")
        .or_else(|| parent.get("name"))
        .or_else(|| parent.get("id"))
        .cloned()
        .unwrap_or_else(|| parent.clone());
    let child_value = child
        .get("value")
        .or_else(|| child.get("name"))
        .or_else(|| child.get("id"))
        .cloned()
        .unwrap_or_else(|| child.clone());
    serde_json::json!({"value": parent_value, "child": {"value": child_value}})
}
fn choice_entries(field: &JiraField) -> Vec<(String, String, Value)> {
    field
        .allowed
        .iter()
        .flat_map(|parent| {
            let parent_key = choice_key(field, parent);
            let parent_label = choice_name(parent);
            parent["children"]
                .as_array()
                .filter(|children| !children.is_empty())
                .map(|children| {
                    children
                        .iter()
                        .map(|child| {
                            let child_key = choice_key(field, child);
                            (
                                format!("{parent_key}::{child_key}"),
                                format!("{parent_label} / {}", choice_name(child)),
                                cascading_value(parent, child),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![(parent_key, parent_label, choice_value(field, parent))])
        })
        .collect()
}
fn selected_choice_value(field: &JiraField, value: &Value) -> Value {
    if value.get("child").is_some() {
        value.clone()
    } else {
        choice_value(field, value)
    }
}
fn reference_field(field: &JiraField) -> bool {
    let kind = field.schema["type"].as_str().unwrap_or("");
    let item_kind = field.schema["items"].as_str().unwrap_or("");
    matches!(
        kind,
        "user"
            | "issuelink"
            | "option"
            | "priority"
            | "resolution"
            | "issuetype"
            | "project"
            | "version"
            | "component"
            | "status"
    ) || matches!(
        item_kind,
        "user" | "option" | "version" | "component" | "issuelink"
    ) || field.id == "parent"
}
impl FieldInput {
    pub fn new(
        field: JiraField,
        value: Value,
        base: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kind = field.schema["type"].as_str().unwrap_or("");
        let mut events = vec![];
        let mut selected = vec![];
        let initial = if value.is_null() {
            String::new()
        } else if value["type"] == "doc" {
            adf::editing_text(&value)
        } else if let Some(s) = value.as_str() {
            s.into()
        } else if kind == "array" && field.schema["items"] == "string" {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        } else if kind == "user" {
            value["displayName"].as_str().unwrap_or("").into()
        } else if field.id == "parent" {
            value["key"].as_str().unwrap_or("").into()
        } else {
            value.to_string()
        };
        let mut choice_options = vec![];
        let has_choices = !(matches!(
            field.id.as_str(),
            "description" | "environment" | "_comment"
        ) || (field.allowed.is_empty() && !reference_field(&field))
            || (kind == "array" && field.schema["items"] == "string"));
        let control = if kind == "boolean" {
            selected = vec![Value::Bool(value.as_bool().unwrap_or(false))];
            Control::Boolean
        } else if has_choices {
            let entries = choice_entries(&field);
            let current_values = if kind == "array" {
                value.as_array().cloned().unwrap_or_default()
            } else if value.is_null() {
                Vec::new()
            } else {
                vec![value.clone()]
            };
            selected = current_values
                .iter()
                .map(|value| selected_choice_value(&field, value))
                .collect();
            choice_options = entries
                .iter()
                .map(|(key, label, raw)| (key.clone(), label.clone(), raw.clone()))
                .collect();
            for (raw, selected) in current_values.iter().zip(selected.iter()) {
                let key = choice_key(&field, selected);
                if !choice_options
                    .iter()
                    .any(|(candidate, _, _)| candidate == &key)
                {
                    choice_options.push((key, selected_choice_label(raw), selected.clone()));
                }
            }
            let choices = choice_options
                .iter()
                .map(|(key, label, _)| SelectOption::new(key.clone(), label.clone()))
                .collect::<Vec<_>>();
            let selected_keys = selected
                .iter()
                .map(|value| choice_key(&field, value).into())
                .collect::<Vec<_>>();
            let state = cx.new(|cx| {
                ComboboxState::new(SearchableVec::new(choices), vec![], window, cx)
                    .multiple(kind == "array")
                    .searchable(true)
            });
            state.update(cx, |select, cx| {
                select.set_selected_values(&selected_keys, window, cx)
            });
            events.push(cx.subscribe(
                &state,
                |s, _, event: &ComboboxEvent<SearchableVec<SelectOption>>, cx| {
                    if let ComboboxEvent::Change(ids) = event {
                        s.selected = ids
                            .iter()
                            .filter_map(|id| {
                                s.choice_options
                                    .iter()
                                    .find(|(key, _, _)| key == id.as_ref())
                                    .map(|(_, _, option)| option.clone())
                            })
                            .collect();
                        cx.emit(FieldChanged);
                        cx.notify();
                    }
                },
            ));
            Control::Choices(state)
        } else if matches!(
            field.id.as_str(),
            "description" | "environment" | "_comment"
        ) || field.schema["custom"]
            .as_str()
            .is_some_and(|s| s.ends_with(":textarea"))
        {
            let input = cx.new(|cx| MarkdownComposer::new(base, 220., window, cx));
            input.update(cx, |s, cx| s.set_value(&initial, window, cx));
            events.push(cx.subscribe(&input, |_, _, _: &ComposerChanged, cx| {
                cx.emit(FieldChanged);
                cx.notify();
            }));
            Control::Markdown(input)
        } else if matches!(
            kind,
            "string" | "number" | "date" | "datetime" | "boolean" | "user"
        ) || field.id == "parent"
            || kind == "array" && field.schema["items"] == "string"
        {
            let input = cx.new(|cx| InputState::new(window, cx));
            input.update(cx, |s, cx| s.set_value(initial.clone(), window, cx));
            events.push(cx.subscribe(&input, |_, _, _: &InputEvent, cx| {
                cx.emit(FieldChanged);
                cx.notify();
            }));
            Control::Text(input)
        } else {
            let input = cx.new(|cx| TextareaState::new(window, cx));
            input.update(cx, |s, cx| {
                s.set_value(
                    if value.is_null() {
                        String::new()
                    } else {
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    },
                    window,
                    cx,
                )
            });
            events.push(cx.subscribe(&input, |_, _, _: &InputEvent, cx| {
                cx.emit(FieldChanged);
                cx.notify();
            }));
            Control::Structured(input)
        };
        let mut s = Self {
            field,
            baseline: value,
            selected,
            choice_options,
            control,
            initial,
            initial_draft: Value::Null,
            enabled: true,
            _events: events,
        };
        s.initial_draft = s.draft(cx);
        s
    }
    pub fn dirty(&self, cx: &App) -> bool {
        self.draft(cx) != self.initial_draft
    }
    pub fn enable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.enabled = value;
        if let Control::Markdown(m) = &self.control {
            m.update(cx, |s, cx| s.enable(value, cx));
        }
        cx.notify();
    }
    pub fn value(&self, cx: &App) -> Result<Value, String> {
        match &self.control {
            Control::Boolean => Ok(self.selected.first().cloned().unwrap_or(Value::Bool(false))),
            Control::Text(s) => {
                let text = s.read(cx).value();
                if text.as_ref() == self.initial {
                    Ok(self.baseline.clone())
                } else {
                    field_value(&self.field, &text)
                }
            }
            Control::Markdown(s) => {
                let text = s.read(cx).value(cx);
                if text == self.initial {
                    Ok(self.baseline.clone())
                } else if self.field.id == "_comment" {
                    Ok(Value::String(text))
                } else {
                    adf::from_editing_text(&text, &self.baseline)
                }
            }
            Control::Structured(s) => {
                let text = s.read(cx).value();
                if text.trim().is_empty() {
                    return if self.field.required {
                        Err(format!("{} is required.", self.field.name))
                    } else {
                        Ok(Value::Null)
                    };
                }
                serde_json::from_str(&text)
                    .map_err(|_| format!("{} contains invalid JSON.", self.field.name))
            }
            Control::Choices(_) => {
                if self.field.schema["type"] == "array" {
                    Ok(Value::Array(self.selected.clone()))
                } else {
                    Ok(self.selected.first().cloned().unwrap_or(Value::Null))
                }
            }
        }
    }
    pub fn draft(&self, cx: &App) -> Value {
        // Preserve invalid in-progress text without attempting a network write.
        match &self.control {
            Control::Boolean => {
                serde_json::json!({"boolean":self.selected.first().and_then(Value::as_bool).unwrap_or(false)})
            }
            Control::Text(s) => serde_json::json!({"text":s.read(cx).value().to_string()}),
            Control::Markdown(s) => {
                serde_json::json!({"text":s.read(cx).value(cx),"original":self.baseline})
            }
            Control::Structured(s) => serde_json::json!({"json":s.read(cx).value().to_string()}),
            Control::Choices(_) => serde_json::json!({"choices":self.selected}),
        }
    }
    pub fn restore(&mut self, v: &Value, window: &mut Window, cx: &mut Context<Self>) {
        match &self.control {
            Control::Boolean => {
                if let Some(value) = v["boolean"].as_bool() {
                    self.selected = vec![Value::Bool(value)];
                }
            }
            Control::Text(s) => {
                if let Some(v) = v["text"].as_str() {
                    s.update(cx, |s, cx| s.set_value(v.to_owned(), window, cx));
                }
            }
            Control::Markdown(s) => {
                if let Some(original) = v.get("original") {
                    self.baseline = original.clone();
                    self.initial = adf::editing_text(original);
                }
                if let Some(v) = v["text"].as_str() {
                    s.update(cx, |s, cx| s.set_value(v, window, cx));
                }
            }
            Control::Structured(s) => {
                if let Some(v) = v["json"].as_str() {
                    s.update(cx, |s, cx| s.set_value(v.to_owned(), window, cx));
                }
            }
            Control::Choices(_) => {
                if let Some(v) = v["choices"].as_array() {
                    self.selected = v.clone();
                    if let Control::Choices(select) = &self.control {
                        let keys = self
                            .selected
                            .iter()
                            .map(|value| choice_key(&self.field, value).into())
                            .collect::<Vec<_>>();
                        select.update(cx, |select, cx| {
                            select.set_selected_values(&keys, window, cx)
                        });
                    }
                }
            }
        }
    }
}
impl Render for FieldInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = format!(
            "{}{}",
            self.field.name,
            if self.field.required { " *" } else { "" }
        );
        let help = if matches!(self.control, Control::Markdown(_)) && !adf::editable(&self.baseline)
        {
            "Jira mentions and embedded content appear as links while editing. Keep each content link unchanged to preserve it."
        } else if matches!(self.control, Control::Structured(_)) {
            "Advanced structured field. The original document is preserved, including rich Jira content."
        } else if self.field.schema["type"] == "date" {
            "YYYY-MM-DD"
        } else if self.field.schema["type"] == "datetime" {
            "YYYY-MM-DD HH:MM (local time)"
        } else if self.field.schema["type"] == "array" && self.field.schema["items"] == "string" {
            "Separate values with commas."
        } else if reference_field(&self.field) && self.field.allowed.is_empty() {
            "No options were returned for this field. Check Jira permissions and retry."
        } else {
            ""
        };
        let content = match &self.control {
            Control::Boolean => checkbox(
                "jira-boolean",
                self.selected
                    .first()
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                self.field.name.clone(),
            )
            .disabled(!self.enabled)
            .on_click(cx.listener(|s, value, _, cx| {
                s.selected = vec![Value::Bool(*value)];
                cx.emit(FieldChanged);
                cx.notify();
            }))
            .into_any_element(),
            Control::Text(s) => input(s).w_full().disabled(!self.enabled).into_any_element(),
            Control::Markdown(s) => s.clone().into_any_element(),
            Control::Structured(s) => textarea(s)
                .w_full()
                .h(px(180.))
                .font_family(t::MONO)
                .disabled(!self.enabled)
                .into_any_element(),
            Control::Choices(s) => {
                let multi = self.field.schema["type"] == "array";
                let placeholder = if self.field.schema["type"] == "user"
                    || self.field.schema["items"] == "user"
                {
                    "Choose a person"
                } else if self.field.id == "parent" || self.field.schema["type"] == "issuelink" {
                    "Choose an issue"
                } else {
                    "Choose…"
                };
                combobox(s)
                    .w_full()
                    .placeholder(placeholder)
                    .cleanable(!multi && !self.field.required)
                    .disabled(!self.enabled)
                    .into_any_element()
            }
        };
        form_field(title, help, content)
    }
}
