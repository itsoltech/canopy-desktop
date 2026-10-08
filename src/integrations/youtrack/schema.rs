use super::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoutrackFieldKind {
    String,
    Text,
    Date,
    Integer,
    Float,
    Boolean,
    Enum,
    State,
    StateMachine,
    User,
    Group,
    Version,
    Build,
    OwnedField,
    Period,
    Unsupported,
}

#[derive(Clone, Debug)]
pub struct YoutrackField {
    pub id: String,
    pub name: String,
    pub field_type: String,
    pub field_type_id: String,
    pub kind: YoutrackFieldKind,
    pub required: bool,
    pub read_only: bool,
    pub multi_value: bool,
    pub bundle_id: Option<String>,
    pub value: Value,
    pub default: Value,
    pub allowed: Vec<Value>,
    pub events: Vec<TaskOption>,
}

impl YoutrackField {
    /// Overlay an issue's current value without replacing project metadata.
    ///
    /// Project custom-field metadata owns the bundle/options and editability
    /// contract. Issue responses often contain only the selected value (and
    /// therefore an empty bundle), so replacing `allowed` here makes a later
    /// form render an ID text box or an empty picker. Keep useful options from
    /// a complete issue response only when the project response did not have
    /// them at all.
    pub fn with_current_value(mut self, current: &Self) -> Self {
        self.value = current.value.clone();
        // An issue can specialize a project's state field into a state machine.
        // Its runtime type wins; a bundle must never bypass the workflow events.
        if current.kind != YoutrackFieldKind::Unsupported {
            self.kind = current.kind;
            self.field_type = current.field_type.clone();
        }
        if self.field_type_id.is_empty() {
            self.field_type_id = current.field_type_id.clone();
        }
        self.read_only |= current.read_only;
        if self.allowed.is_empty() && !current.allowed.is_empty() {
            self.allowed = current.allowed.clone();
        }
        self.events = current.events.clone();
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct YoutrackSchema {
    pub fields: Vec<YoutrackField>,
    pub next_cursor: Option<String>,
}

pub(super) fn field_kind(field_type_id: &str, field_type: &str) -> YoutrackFieldKind {
    // The schema uses ids such as `state[1]`, not Rust/REST class names.
    // State-machine specialization is only present on the issue's $type.
    if field_type == "StateMachineIssueCustomField" {
        return YoutrackFieldKind::StateMachine;
    }
    let lower = if field_type_id == "unsupported" || field_type_id.is_empty() {
        field_type.to_ascii_lowercase()
    } else {
        field_type_id.to_ascii_lowercase()
    };
    if lower.contains("statemachine") {
        YoutrackFieldKind::StateMachine
    } else if lower.starts_with("state") || lower.starts_with("multistate") {
        YoutrackFieldKind::State
    } else if lower.starts_with("text") {
        YoutrackFieldKind::Text
    } else if lower.starts_with("date") {
        YoutrackFieldKind::Date
    } else if lower.contains("integer") {
        YoutrackFieldKind::Integer
    } else if lower.contains("float") {
        YoutrackFieldKind::Float
    } else if lower.contains("boolean") {
        YoutrackFieldKind::Boolean
    } else if lower.contains("enum") {
        YoutrackFieldKind::Enum
    } else if lower.contains("user") {
        YoutrackFieldKind::User
    } else if lower.contains("group") {
        YoutrackFieldKind::Group
    } else if lower.contains("version") {
        YoutrackFieldKind::Version
    } else if lower.contains("build") {
        YoutrackFieldKind::Build
    } else if lower.contains("owned") {
        YoutrackFieldKind::OwnedField
    } else if lower.contains("period") {
        YoutrackFieldKind::Period
    } else if lower.contains("string") {
        YoutrackFieldKind::String
    } else {
        YoutrackFieldKind::Unsupported
    }
}

pub fn issue_field_type(field_type_id: &str) -> String {
    let lower = field_type_id.to_ascii_lowercase();
    let multi = lower.contains("[*]") || lower.ends_with("[]");
    let cardinality = if multi { "Multi" } else { "Single" };
    if lower.contains("statemachine") {
        "StateMachineIssueCustomField".into()
    } else if lower.starts_with("enum") {
        format!("{cardinality}EnumIssueCustomField")
    } else if lower.starts_with("user") {
        format!("{cardinality}UserIssueCustomField")
    } else if lower.starts_with("group") {
        format!("{cardinality}GroupIssueCustomField")
    } else if lower.starts_with("version") {
        format!("{cardinality}VersionIssueCustomField")
    } else if lower.starts_with("owned") {
        format!("{cardinality}OwnedFieldIssueCustomField")
    } else if lower.starts_with("state") {
        if multi {
            "MultiStateIssueCustomField".into()
        } else {
            "StateIssueCustomField".into()
        }
    } else if lower.starts_with("text") {
        "TextIssueCustomField".into()
    } else if lower.starts_with("date") {
        "DateIssueCustomField".into()
    } else if lower.starts_with("period") {
        "PeriodIssueCustomField".into()
    } else if lower.starts_with("float") {
        "FloatIssueCustomField".into()
    } else if lower.starts_with("integer") {
        "IntegerIssueCustomField".into()
    } else if lower.starts_with("boolean") {
        "BooleanIssueCustomField".into()
    } else {
        "SimpleIssueCustomField".into()
    }
}

fn is_multi(value: &Value) -> bool {
    value["fieldType"]["isMultiValue"] == true
        || value["field"]["fieldType"]["isMultiValue"] == true
        || value["fieldType"]["id"]
            .as_str()
            .is_some_and(|id| id.ends_with("[*]") || id.ends_with("[]"))
        || value["field"]["fieldType"]["id"]
            .as_str()
            .is_some_and(|id| id.ends_with("[*]") || id.ends_with("[]"))
        || value["value"].is_array()
}

fn options(value: &Value) -> Vec<Value> {
    let mut values = value["bundle"]["values"]
        .as_array()
        .or_else(|| value["values"].as_array())
        .cloned()
        .unwrap_or_default();
    for key in ["aggregatedUsers", "users", "groups"] {
        if let Some(extra) = value[key].as_array() {
            values.extend(extra.iter().cloned());
        }
        if let Some(extra) = value["bundle"][key].as_array() {
            values.extend(extra.iter().cloned());
        }
    }
    values
        .into_iter()
        .filter(|v| v["archived"] != true)
        .collect()
}

fn parse_field(value: &Value) -> Option<YoutrackField> {
    let field = value.get("field").unwrap_or(value);
    let id = value["id"].as_str().or(field["id"].as_str())?.to_owned();
    let name = value["name"]
        .as_str()
        .or(field["name"].as_str())
        .unwrap_or(&id)
        .to_owned();
    let field_type_id = value["fieldType"]["id"]
        .as_str()
        .or(field["fieldType"]["id"].as_str())
        .unwrap_or("unsupported")
        .to_owned();
    let field_type = value["$type"]
        .as_str()
        .filter(|kind| kind.contains("IssueCustomField"))
        .map(str::to_owned)
        .unwrap_or_else(|| issue_field_type(&field_type_id));
    let kind = field_kind(&field_type_id, &field_type);
    let allowed = options(value);
    let required = value["canBeEmpty"] == false || value["required"] == true;
    let read_only = value["readOnly"] == true || field["readOnly"] == true;
    Some(YoutrackField {
        id,
        name,
        field_type,
        field_type_id,
        kind,
        required,
        read_only,
        multi_value: is_multi(value),
        bundle_id: value["bundle"]["id"]
            .as_str()
            .or(value["projectCustomField"]["bundle"]["id"].as_str())
            .map(str::to_owned),
        value: value["value"].clone(),
        default: value["defaultValues"]
            .as_array()
            .and_then(|values| values.first())
            .cloned()
            .or_else(|| (!value["defaultValue"].is_null()).then(|| value["defaultValue"].clone()))
            .unwrap_or(Value::Null),
        allowed,
        events: value["possibleEvents"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|event| {
                Some(TaskOption {
                    value: event["id"].as_str()?.to_owned(),
                    label: event["presentation"]
                        .as_str()
                        .filter(|label| !label.is_empty())
                        .unwrap_or(event["id"].as_str()?)
                        .to_owned(),
                })
            })
            .collect(),
    })
}

pub(super) async fn load(y: &Youtrack, target: &ProjectTarget) -> Result<YoutrackSchema, String> {
    y.target(target)?;
    let project_id = super::read::project_id(y, target).await?;
    let fields_projection = "id,name,$type,canBeEmpty,readOnly,defaultValue,defaultValues,value,field(id,name,fieldType(id,isMultiValue),$type),fieldType(id,isMultiValue),bundle(id,name,$type,values(id,name,login,fullName,isResolved,archived),aggregatedUsers(id,login,name,fullName),groups(id,name)),possibleEvents(id,presentation)";
    let mut fields = Vec::new();
    let mut skip = 0usize;
    loop {
        let value = y
            .get(&format!(
                "/admin/projects/{}/customFields?fields={}&$top=100&$skip={skip}",
                super::transport::component(&project_id),
                encoded_query_component(fields_projection),
            ))
            .await?;
        let rows = value
            .as_array()
            .or_else(|| value["values"].as_array())
            .ok_or("YouTrack returned invalid project field metadata.")?;
        let raw_count = rows.len();
        for row in rows {
            if let Some(mut field) = parse_field(row) {
                if let Err(error) = load_bundle_values(y, &mut field).await
                    && field.allowed.is_empty()
                {
                    return Err(error);
                }
                fields.push(field);
            }
        }
        if raw_count < 100 {
            break;
        }
        skip += raw_count;
        if skip > 10_000 {
            return Err("YouTrack returned too many project fields.".into());
        }
    }
    Ok(YoutrackSchema {
        fields,
        next_cursor: None,
    })
}

pub(super) async fn load_bundle_values(
    y: &Youtrack,
    field: &mut YoutrackField,
) -> Result<(), String> {
    let Some(bundle_id) = field.bundle_id.clone() else {
        return Ok(());
    };
    super::validate_identifier(&bundle_id, "bundle ID")?;
    let lower = field.field_type_id.to_ascii_lowercase();
    let (resource, projection) = if lower.contains("user") {
        (
            format!("/admin/customFieldSettings/bundles/user/{bundle_id}/aggregatedUsers"),
            "id,login,name,fullName",
        )
    } else if lower.contains("group") {
        (
            format!("/admin/customFieldSettings/bundles/user/{bundle_id}/groups"),
            "id,name",
        )
    } else {
        let kind = if lower.contains("enum") {
            "enum"
        } else if field.kind == YoutrackFieldKind::State {
            "state"
        } else if lower.contains("version") {
            "version"
        } else if lower.contains("owned") {
            "ownedField"
        } else if lower.contains("build") {
            "build"
        } else {
            return Ok(());
        };
        (
            format!("/admin/customFieldSettings/bundles/{kind}/{bundle_id}/values"),
            if field.kind == YoutrackFieldKind::State {
                "id,name,isResolved,archived"
            } else {
                "id,name,login,fullName,isResolved,archived"
            },
        )
    };
    let mut skip = 0usize;
    let mut values = Vec::new();
    loop {
        let response = y
            .get(&format!(
                "{resource}?fields={}&$top=100&$skip={skip}",
                super::transport::component(projection)
            ))
            .await?;
        let rows = response
            .as_array()
            .or_else(|| response["values"].as_array())
            .or_else(|| response["users"].as_array())
            .ok_or("YouTrack returned invalid custom-field bundle values.")?;
        let raw_count = rows.len();
        values.extend(rows.iter().filter(|v| v["archived"] != true).cloned());
        if raw_count < 100 {
            break;
        }
        skip += raw_count;
        if skip > 100_000 {
            return Err("YouTrack returned too many custom-field values.".into());
        }
    }
    field.allowed = values;
    Ok(())
}

fn encoded_query_component(value: &str) -> String {
    super::transport::component(value)
}

pub fn value_for_field(field: &YoutrackField, text: &str) -> Result<Value, String> {
    let text = text.trim();
    if text.is_empty() {
        if field.required && field.default.is_null() {
            return Err(format!("{} is required.", field.name));
        }
        return Ok(Value::Null);
    }
    match field.kind {
        YoutrackFieldKind::Integer | YoutrackFieldKind::Period => text
            .parse::<i64>()
            .map(|n| {
                if field.kind == YoutrackFieldKind::Period {
                    serde_json::json!({"minutes": n})
                } else {
                    Value::Number(n.into())
                }
            })
            .map_err(|_| format!("{} must be a whole number.", field.name)),
        YoutrackFieldKind::Float => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| format!("{} must be a number.", field.name)),
        YoutrackFieldKind::Boolean => text
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_| format!("{} must be true or false.", field.name)),
        YoutrackFieldKind::Date => chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map(|date| {
                Value::Number(
                    (date
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis())
                    .into(),
                )
            })
            .map_err(|_| format!("Use YYYY-MM-DD for {}.", field.name)),
        YoutrackFieldKind::User => Ok(serde_json::json!({"login": text})),
        YoutrackFieldKind::Group => Ok(serde_json::json!({"id": text})),
        YoutrackFieldKind::Enum
        | YoutrackFieldKind::State
        | YoutrackFieldKind::Version
        | YoutrackFieldKind::Build
        | YoutrackFieldKind::OwnedField => Ok(serde_json::json!({"name": text})),
        YoutrackFieldKind::String => Ok(Value::String(text.into())),
        YoutrackFieldKind::Text => Ok(serde_json::json!({"text": text})),
        YoutrackFieldKind::StateMachine | YoutrackFieldKind::Unsupported => {
            serde_json::from_str(text)
                .map_err(|_| format!("{} requires a structured value.", field.name))
        }
    }
}
