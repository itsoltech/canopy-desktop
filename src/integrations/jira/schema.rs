use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaRequest {
    Projects {
        cursor: Option<String>,
    },
    Types,
    Create {
        issue_type: String,
    },
    Edit {
        key: String,
    },
    Transitions {
        key: String,
    },
    LinkTypes,
    Boards {
        cursor: Option<String>,
    },
    Sprints {
        board: String,
        cursor: Option<String>,
    },
    History {
        key: String,
        cursor: Option<String>,
    },
}
#[derive(Clone, Debug)]
pub struct JiraField {
    pub id: String,
    pub name: String,
    pub required: bool,
    pub schema: Value,
    pub allowed: Vec<Value>,
    pub default: Value,
}
#[derive(Clone, Debug)]
pub struct JiraTransition {
    pub id: String,
    pub name: String,
    pub destination: String,
    pub fields: Vec<JiraField>,
}
impl JiraTransition {
    pub fn requires_form(&self) -> bool {
        // A default is not consent to submit a required resolution/other field.
        self.fields.iter().any(|field| field.required)
    }

    pub fn label(&self) -> String {
        if self.destination.is_empty() {
            self.name.clone()
        } else if self.name.is_empty() || self.name == self.destination {
            self.destination.clone()
        } else {
            format!("{} — {}", self.destination, self.name)
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct JiraSchema {
    pub fields: Vec<JiraField>,
    pub options: Vec<TaskOption>,
    pub next_cursor: Option<String>,
    pub transitions: Vec<JiraTransition>,
    pub activity: Vec<String>,
}
fn field(id: &str, v: &Value) -> JiraField {
    JiraField {
        id: id.into(),
        name: v["name"].as_str().unwrap_or(id).into(),
        required: v["required"] == true,
        schema: v["schema"].clone(),
        allowed: v["allowedValues"].as_array().cloned().unwrap_or_default(),
        default: v["defaultValue"].clone(),
    }
}
fn fields(v: &Value) -> Vec<JiraField> {
    v.as_object()
        .into_iter()
        .flatten()
        .map(|(id, v)| field(id, v))
        .collect()
}
pub(super) async fn load(
    j: &Jira,
    target: &ProjectTarget,
    request: &SchemaRequest,
) -> Result<JiraSchema, String> {
    j.target(target)?;
    let mut result = JiraSchema::default();
    match request {
        SchemaRequest::Projects { cursor } => {
            let p = j.projects(cursor.as_deref()).await?;
            result.options = p.items;
            result.next_cursor = p.next_cursor;
        }
        SchemaRequest::Types => {
            let mut start = 0;
            loop {
                let v = j
                    .get(&format!(
                        "/rest/api/3/issue/createmeta/{}/issuetypes?startAt={start}&maxResults=100",
                        target.key
                    ))
                    .await?;
                let items = v["issueTypes"]
                    .as_array()
                    .or_else(|| v["values"].as_array())
                    .ok_or("Jira returned no create types. Check Create issues permission.")?;
                result.options.extend(items.iter().filter_map(|v| {
                    Some(TaskOption {
                        value: v["id"].as_str()?.into(),
                        label: v["name"].as_str()?.into(),
                    })
                }));
                start += items.len();
                if items.is_empty()
                    || v["isLast"] == true
                    || start >= v["total"].as_u64().unwrap_or(start as u64) as usize
                {
                    break;
                }
                if start >= 2000 {
                    return Err("Too many Jira issue types.".into());
                }
            }
        }
        SchemaRequest::Create { issue_type } => {
            ident(issue_type)?;
            let mut start = 0;
            loop {
                let v=j.get(&format!("/rest/api/3/issue/createmeta/{}/issuetypes/{issue_type}?startAt={start}&maxResults=100",target.key)).await?;
                let items = v["fields"]
                    .as_array()
                    .or_else(|| v["values"].as_array())
                    .ok_or("Jira returned no create field metadata.")?;
                result.fields.extend(items.iter().filter_map(|v| {
                    v["fieldId"]
                        .as_str()
                        .or(v["key"].as_str())
                        .map(|id| field(id, v))
                }));
                start += items.len();
                if items.is_empty()
                    || v["isLast"] == true
                    || start >= v["total"].as_u64().unwrap_or(start as u64) as usize
                {
                    break;
                }
                if start >= 1000 {
                    return Err("Too many Jira create fields.".into());
                }
            }
        }
        SchemaRequest::Edit { key } | SchemaRequest::Transitions { key } => {
            let task = TaskRef {
                project: target.clone(),
                id: key.clone(),
                title: String::new(),
            };
            let path = issue_path(&task)?;
            if matches!(request, SchemaRequest::Edit { .. }) {
                let v = j.get(&format!("{path}/editmeta")).await?;
                result.fields = fields(&v["fields"]);
            } else {
                let v = j
                    .get(&format!("{path}/transitions?expand=transitions.fields"))
                    .await?;
                let ts = v["transitions"]
                    .as_array()
                    .ok_or("Jira returned invalid transitions.")?;
                result.transitions = ts
                    .iter()
                    .filter(|transition| transition["isAvailable"] != false)
                    .map(|t| {
                        // Missing/malformed expanded metadata must not authorize a quick write.
                        let id = t["id"].as_str().ok_or("Jira returned a transition without an ID.")?;
                        ident(id)?;
                        let metadata = t["fields"].as_object().ok_or("Jira returned no transition field metadata. Refresh the status choices.")?;
                        if metadata.values().any(|field| field["required"].as_bool().is_none()) {
                            return Err("Jira returned incomplete transition field requirements.".into());
                        }
                        Ok(JiraTransition {
                            id: id.into(),
                            name: t["name"].as_str().unwrap_or("").into(),
                            destination: t["to"]["name"].as_str().unwrap_or("").into(),
                            fields: fields(&t["fields"]),
                        })
                    })
                    .collect::<Result<_, String>>()?;
            }
        }
        SchemaRequest::LinkTypes => {
            let v = j.get("/rest/api/3/issueLinkType").await?;
            result.options = v["issueLinkTypes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| {
                    Some(TaskOption {
                        value: l["name"].as_str()?.into(),
                        label: format!(
                            "{} / {}",
                            l["outward"].as_str().unwrap_or(""),
                            l["inward"].as_str().unwrap_or("")
                        ),
                    })
                })
                .collect();
        }
        SchemaRequest::Boards { cursor } | SchemaRequest::Sprints { cursor, .. } => {
            let start = read::offset(cursor.as_deref())?;
            let path = match request {
                SchemaRequest::Boards { .. } => format!(
                    "/rest/agile/1.0/board?projectKeyOrId={}&startAt={start}&maxResults=50",
                    target.key
                ),
                SchemaRequest::Sprints { board, .. } => format!(
                    "/rest/agile/1.0/board/{}/sprint?state=active,future&startAt={start}&maxResults=50",
                    ident(board)?
                ),
                _ => unreachable!(),
            };
            let v = j.get(&path).await?;
            let items = v["values"]
                .as_array()
                .ok_or("No Jira board / sprint values returned.")?;
            result.options = items
                .iter()
                .filter_map(|v| {
                    Some(TaskOption {
                        value: v["id"].as_u64()?.to_string(),
                        label: v["name"].as_str()?.into(),
                    })
                })
                .collect();
            result.next_cursor = (v["isLast"] != true && !items.is_empty())
                .then(|| (start + items.len()).to_string());
        }
        SchemaRequest::History { key, cursor } => {
            let task = TaskRef {
                project: target.clone(),
                id: key.clone(),
                title: String::new(),
            };
            let start = read::offset(cursor.as_deref())?;
            let v = j
                .get(&format!(
                    "{}/changelog?startAt={start}&maxResults=30",
                    issue_path(&task)?
                ))
                .await?;
            let rows = v["values"]
                .as_array()
                .ok_or("Invalid Jira activity response.")?;
            for r in rows {
                for i in r["items"].as_array().into_iter().flatten() {
                    result.activity.push(format!(
                        "{} · {}\n{}: {} → {}",
                        r["author"]["displayName"].as_str().unwrap_or("User"),
                        r["created"].as_str().unwrap_or(""),
                        i["field"].as_str().unwrap_or("Field"),
                        i["fromString"].as_str().unwrap_or("—"),
                        i["toString"].as_str().unwrap_or("—")
                    ));
                }
            }
            result.next_cursor = (v["isLast"] != true
                && start + rows.len() < v["total"].as_u64().unwrap_or(0) as usize
                && !rows.is_empty())
            .then(|| (start + rows.len()).to_string());
        }
    }
    // Jira exposes allowed values for many built-in fields only through a
    // separate lookup.  Keep the field a picker even when editmeta did not
    // inline those values; the UI can then use the provider identity while
    // showing a human-readable label.
    if matches!(
        request,
        SchemaRequest::Create { .. } | SchemaRequest::Edit { .. }
    ) {
        let mut users = None;
        let mut issues = None;
        for field in &mut result.fields {
            if !field.allowed.is_empty() {
                continue;
            }
            if field.schema["type"] == "user" || field.schema["items"] == "user" {
                if users.is_none() {
                    let value = j
                        .get(&format!(
                            "/rest/api/3/user/assignable/search?project={}&startAt=0&maxResults=100",
                            target.key
                        ))
                        .await?;
                    users = Some(
                        value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|user| user["accountId"].as_str().is_some())
                            .cloned()
                            .collect::<Vec<_>>(),
                    );
                }
                field.allowed = users.clone().unwrap_or_default();
            } else if field.id == "parent" || field.schema["type"] == "issuelink" {
                if issues.is_none() {
                    let jql = format!("project = \"{}\" ORDER BY updated DESC", target.key);
                    let value = j
                        .send(
                            "POST",
                            "/rest/api/3/search/jql",
                            Some(serde_json::json!({
                                "jql": jql,
                                "maxResults": 50,
                                "fields": ["summary"]
                            })),
                            false,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    issues = Some(
                        value["issues"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|issue| issue["key"].as_str().is_some())
                            .cloned()
                            .collect::<Vec<_>>(),
                    );
                }
                field.allowed = issues.clone().unwrap_or_default();
            }
        }
    }
    result.fields.sort_by_key(|f| (!f.required, f.name.clone()));
    Ok(result)
}
/// Encode only explicitly edited values, respecting the field's Jira schema.
pub fn field_value(field: &JiraField, text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return if field.required {
            Err(format!("{} is required.", field.name))
        } else {
            Ok(Value::Null)
        };
    }
    let kind = field.schema["type"].as_str().unwrap_or("");
    let custom = field.schema["custom"].as_str().unwrap_or("");
    match kind {
        "number" => serde_json::from_str::<Value>(text)
            .ok()
            .filter(Value::is_number)
            .ok_or_else(|| format!("{} must be a number.", field.name)),
        "string"
            if custom.ends_with(":textarea")
                || matches!(field.id.as_str(), "description" | "environment") =>
        {
            adf::from_markdown(text)
        }
        "string" => Ok(Value::String(text.into())),
        "date" => chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
            .map_err(|_| format!("Use a valid YYYY-MM-DD date for {}.", field.name)),
        "datetime" => date_time(text).map(Value::String),
        "boolean" => text
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_| format!("Use true or false for {}.", field.name)),
        "array" if field.schema["items"] == "string" => Ok(serde_json::json!(
            text.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        )),
        "user" => Ok(serde_json::json!({"accountId":text.trim()})),
        "issuelink" if field.id == "parent" => {
            if valid_issue_key(text.trim()) {
                Ok(serde_json::json!({"key":text.trim()}))
            } else {
                Err("Enter the parent issue key.".into())
            }
        }
        _ => serde_json::from_str(text).map_err(|_| {
            format!(
                "{} requires a JSON value matching its Jira field type.",
                field.name
            )
        }),
    }
}
pub fn choice_value(field: &JiraField, value: &Value) -> Value {
    if value.is_string() {
        return value.clone();
    }
    if field.schema["type"] == "user" || field.schema["items"] == "user" {
        return serde_json::json!({"accountId":value["accountId"]});
    }
    if !value["key"].is_null() {
        return serde_json::json!({"key":value["key"]});
    }
    if !value["id"].is_null() {
        return serde_json::json!({"id":value["id"]});
    }
    if !value["value"].is_null() {
        return serde_json::json!({"value":value["value"]});
    }
    serde_json::json!({"name":value["name"]})
}

/// Accept a local wall-clock value or a timestamp with an explicit offset.
pub fn date_time(text: &str) -> Result<String, String> {
    use chrono::TimeZone;
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text)
        .or_else(|_| chrono::DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.3f%z"))
    {
        return Ok(dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string());
    }
    let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
        .map_err(|_| "Use YYYY-MM-DD HH:MM or a date with an explicit timezone.")?;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string())
        .ok_or("This local time is ambiguous or unavailable. Include an explicit timezone.".into())
}
