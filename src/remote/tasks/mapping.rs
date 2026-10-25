use super::*;
use prost_types::{ListValue, Struct, Value, value::Kind};
use serde_json::Value as Json;

pub(super) fn json(value: &Value) -> Result<Json, Status> {
    fn convert(value: &Value, depth: usize) -> Result<Json, Status> {
        if depth > 32 {
            return Err(invalid("Task field nesting exceeds 32 levels."));
        }
        Ok(match value.kind.as_ref() {
            None | Some(Kind::NullValue(_)) => Json::Null,
            Some(Kind::BoolValue(v)) => Json::Bool(*v),
            Some(Kind::NumberValue(v)) => {
                // Protobuf Value has doubles, but provider documents (notably
                // ADF's version=1) use integer JSON semantics. Restore exact safe
                // integers; provider identifiers remain opaque strings.
                if v.is_finite() && v.fract() == 0. && v.abs() <= 9_007_199_254_740_991. {
                    Json::Number(serde_json::Number::from(*v as i64))
                } else {
                    Json::Number(
                        serde_json::Number::from_f64(*v)
                            .ok_or_else(|| invalid("Task field numbers must be finite."))?,
                    )
                }
            }
            Some(Kind::StringValue(v)) => Json::String(v.clone()),
            Some(Kind::ListValue(v)) => Json::Array(
                v.values
                    .iter()
                    .map(|v| convert(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            Some(Kind::StructValue(v)) => Json::Object(
                v.fields
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), convert(v, depth + 1)?)))
                    .collect::<Result<_, Status>>()?,
            ),
        })
    }
    if value.encoded_len() > 1024 * 1024 {
        return Err(invalid("Task field exceeds 1 MiB."));
    }
    convert(value, 0)
}
pub(super) fn object(value: Option<&Struct>) -> Result<Json, Status> {
    json(&Value {
        kind: Some(Kind::StructValue(value.cloned().unwrap_or_default())),
    })
}
pub(super) fn value(value: &Json) -> Value {
    Value {
        kind: Some(match value {
            Json::Null => Kind::NullValue(0),
            Json::Bool(v) => Kind::BoolValue(*v),
            Json::Number(v) => Kind::NumberValue(v.as_f64().unwrap_or_default()),
            Json::String(v) => Kind::StringValue(v.clone()),
            Json::Array(v) => Kind::ListValue(ListValue {
                values: v.iter().map(self::value).collect(),
            }),
            Json::Object(v) => Kind::StructValue(Struct {
                fields: v.iter().map(|(k, v)| (k.clone(), self::value(v))).collect(),
            }),
        }),
    }
}
fn structure(v: &Json) -> Option<Struct> {
    match value(v).kind {
        Some(Kind::StructValue(v)) => Some(v),
        _ => None,
    }
}
pub(super) fn option(o: i::TaskOption) -> proto::TaskChoice {
    proto::TaskChoice {
        id: o.value.clone(),
        label: o.label,
        description: None,
        value: Some(value(&Json::String(o.value))),
    }
}
pub(super) fn options(o: i::TaskOptions) -> proto::TaskChoicePage {
    proto::TaskChoicePage {
        choices: o.items.into_iter().map(option).collect(),
        next_page_token: o.next_cursor,
    }
}
fn choice(v: &Json) -> proto::TaskChoice {
    let id = v["id"]
        .as_str()
        .or(v["accountId"].as_str())
        .or(v["name"].as_str())
        .or(v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| v["id"].to_string());
    let label = v["displayName"]
        .as_str()
        .or(v["name"].as_str())
        .or(v["value"].as_str())
        .or(v.as_str())
        .unwrap_or(&id)
        .to_owned();
    proto::TaskChoice {
        id,
        label,
        description: v["description"].as_str().map(str::to_owned),
        value: Some(value(v)),
    }
}
pub(super) fn jira_field(f: i::jira::JiraField) -> proto::TaskField {
    let choices = f
        .allowed
        .iter()
        .flat_map(|parent| {
            let base = choice(parent);
            if let Some(children) = parent["children"].as_array().filter(|c| !c.is_empty()) {
                children
                    .iter()
                    .map(|child| {
                        let c = choice(child);
                        let mut native = i::jira::choice_value(&f, parent);
                        native["child"] = i::jira::choice_value(&f, child);
                        proto::TaskChoice {
                            id: format!("{}::{}", base.id, c.id),
                            label: format!("{} / {}", base.label, c.label),
                            description: c.description,
                            value: Some(value(&native)),
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![proto::TaskChoice {
                    value: Some(value(&i::jira::choice_value(&f, parent))),
                    ..base
                }]
            }
        })
        .collect();
    let kind = if matches!(f.id.as_str(), "description" | "environment")
        || f.schema["custom"]
            .as_str()
            .is_some_and(|s| s.ends_with(":textarea"))
    {
        "object"
    } else {
        f.schema["type"].as_str().unwrap_or("unsupported")
    }
    .to_owned();
    proto::TaskField {
        id: f.id,
        name: f.name,
        kind,
        required: f.required,
        read_only: false,
        multiple: f.schema["type"] == "array",
        choices,
        current_value: None,
        default_value: Some(value(&f.default)),
        provider_schema: structure(&f.schema),
        read_only_reason: None,
    }
}
pub(super) fn youtrack_field(f: i::youtrack::YoutrackField) -> proto::TaskField {
    let schema = serde_json::json!({"$type":f.field_type,"fieldType":f.field_type_id,"bundleId":f.bundle_id});
    proto::TaskField {
        id: f.id,
        name: f.name,
        kind: if f.kind == i::youtrack::YoutrackFieldKind::Text {
            "object".into()
        } else {
            format!("{:?}", f.kind).to_lowercase()
        },
        required: f.required,
        read_only: f.read_only || f.kind == i::youtrack::YoutrackFieldKind::Unsupported,
        multiple: f.multi_value,
        choices: f.allowed.iter().map(choice).collect(),
        current_value: Some(value(&f.value)),
        default_value: Some(value(&f.default)),
        provider_schema: structure(&schema),
        read_only_reason: f.read_only.then(|| "This field is read-only.".into()),
    }
}
pub(super) fn status(f: i::status::StatusField) -> proto::TaskStatusField {
    proto::TaskStatusField {
        id: f.id().into(),
        name: f.name().into(),
        label: f.label(),
        selected_id: f.selected().map(str::to_owned),
        choices: f.choices().into_iter().map(option).collect(),
        read_only_reason: f.read_only_reason().map(str::to_owned),
        hint: f.hint().map(str::to_owned),
        requires_form: matches!(&f,i::status::StatusField::Jira{transitions,..} if transitions.iter().any(|t|t.requires_form())),
    }
}
pub(super) fn comment(c: i::TaskComment) -> proto::TaskComment {
    proto::TaskComment {
        id: c.id,
        author: c.author,
        created_at: c.created_at,
        body: c.body.to_string(),
        url: c.url,
        can_edit: c.can_edit.unwrap_or(false),
        can_delete: c.can_delete.unwrap_or(false),
        rich_body: c.rich_body.as_ref().map(value),
    }
}
pub(super) fn filter(f: i::filters::TaskFilter) -> proto::TaskFilter {
    proto::TaskFilter {
        builtin: i::filters::builtin(&f.id).is_some(),
        id: f.id,
        name: f.name,
        expression: f.expression,
        provider: provider(f.provider).into(),
    }
}
pub(super) fn provider(p: i::Provider) -> &'static str {
    match p {
        i::Provider::Github => "github",
        i::Provider::Jira => "jira",
        i::Provider::Youtrack => "youtrack",
    }
}
pub(super) fn task(item: i::TaskItem, mut reference: proto::TaskRef) -> proto::Task {
    reference.task_id = item.reference.id.clone();
    let mut attachments = vec![];
    if let Some(d) = &item.jira
        && let Some(items) = d.fields["attachment"].as_array()
    {
        attachments.extend(items.iter().filter_map(|a| {
            Some(proto::TaskAttachment {
                id: a["id"].as_str()?.into(),
                name: a["filename"].as_str().unwrap_or("attachment").into(),
                media_type: a["mimeType"]
                    .as_str()
                    .unwrap_or("application/octet-stream")
                    .into(),
                size: a["size"].as_u64().unwrap_or_default(),
                can_delete: true,
            })
        }));
    }
    if let Some(d) = &item.youtrack {
        attachments.extend(d.attachments.iter().map(|a| proto::TaskAttachment {
            id: a.id.clone(),
            name: a.name.clone(),
            media_type: a.mime_type.clone(),
            size: a.size as u64,
            can_delete: true,
        }));
    }
    if item.reference.project.provider == i::Provider::Github
        && let Ok(urls) = i::task_context::github_assets(&item.body)
    {
        attachments.extend(urls.into_iter().map(|url| proto::TaskAttachment {
            id: url.clone(),
            name: url.rsplit('/').next().unwrap_or("attachment").into(),
            media_type: "application/octet-stream".into(),
            size: 0,
            can_delete: false,
        }));
    }
    let p = item.reference.project.provider;
    // Detail values and edit schemas have different roles. Preserve Jira's
    // original IDs/ADF/metadata here; editability and allowed values still come
    // exclusively from ReadForm/ReadEditOptions, never from these read values.
    let fields = if let Some(details) = &item.jira {
        let mut fields = details
            .fields
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, current)| proto::TaskField {
                id: id.clone(),
                name: details.names[id].as_str().unwrap_or(id).into(),
                kind: match current {
                    Json::Array(_) => "array",
                    Json::Object(_) => "object",
                    Json::Bool(_) => "boolean",
                    Json::Number(_) => "number",
                    _ => "string",
                }
                .into(),
                required: false,
                read_only: true,
                multiple: current.is_array(),
                choices: vec![],
                current_value: Some(value(current)),
                default_value: None,
                provider_schema: None,
                read_only_reason: Some(
                    "Detail metadata; use the provider's edit or transition form to change fields."
                        .into(),
                ),
            })
            .collect::<Vec<_>>();
        if !fields.iter().any(|f| f.id == "description") {
            fields.push(proto::TaskField {
                id: "description".into(),
                name: "Description".into(),
                kind: "object".into(),
                read_only: true,
                current_value: Some(value(&details.description)),
                ..Default::default()
            });
        }
        fields.push(proto::TaskField{id:"canopy.jira.metadata".into(),name:"Jira detail metadata".into(),kind:"object".into(),read_only:true,current_value:Some(value(&serde_json::json!({"complete":details.complete,"internal_id":details.internal_id,"description_editable":details.description_editable,"names":details.names}))),..Default::default()});
        fields
    } else {
        item.youtrack
            .as_ref()
            .map(|d| {
                d.custom_fields
                    .iter()
                    .cloned()
                    .map(youtrack_field)
                    .collect()
            })
            .unwrap_or_default()
    };
    let capabilities = [
        ("create", true),
        ("edit", true),
        ("comments", true),
        ("status", true),
        ("labels", true),
        ("assignees", p != i::Provider::Youtrack),
        ("milestone", p == i::Provider::Github),
        ("attachment.read", true),
        ("attachment.write", p != i::Provider::Github),
        ("delete", p != i::Provider::Github),
        ("links", p == i::Provider::Jira),
        ("worklogs", p == i::Provider::Jira),
        ("sprints", p == i::Provider::Jira),
        ("vote", p == i::Provider::Jira),
        ("watch", p == i::Provider::Jira),
    ]
    .into_iter()
    .map(|(operation, supported)| proto::Capability {
        operation: operation.into(),
        supported,
        reason: if supported {
            String::new()
        } else {
            "This operation is not supported by this provider adapter.".into()
        },
    })
    .collect();
    proto::Task {
        task: Some(reference),
        title: item.reference.title.clone(),
        description: item.body.to_string(),
        state: match item.state {
            i::TaskState::Open => "open",
            i::TaskState::Closed => "closed",
        }
        .into(),
        url: item.url.clone(),
        attachments: vec![],
        labels: item
            .youtrack
            .as_ref()
            .map(|d| d.tags.iter().map(|t| t.id.clone()).collect())
            .unwrap_or_else(|| item.labels.clone()),
        assignees: item
            .jira
            .as_ref()
            .map(|d| {
                d.fields["assignee"]["accountId"]
                    .as_str()
                    .map(|id| vec![id.to_owned()])
                    .unwrap_or_default()
            })
            .unwrap_or_else(|| item.assignees.clone()),
        milestone: item
            .milestone
            .as_ref()
            .map(|(id, label)| proto::TaskChoice {
                id: id.to_string(),
                label: label.clone(),
                ..Default::default()
            }),
        statuses: i::status::from_task(&item)
            .into_iter()
            .map(status)
            .collect(),
        fields,
        task_attachments: attachments,
        capabilities,
        links: item
            .jira
            .as_ref()
            .and_then(|details| details.fields["issuelinks"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|link| {
                let outward = link.get("outwardIssue").is_some();
                let other = link.get(if outward {
                    "outwardIssue"
                } else {
                    "inwardIssue"
                })?;
                Some(proto::TaskLink {
                    id: link["id"].as_str()?.into(),
                    kind: link["type"]["name"].as_str().unwrap_or_default().into(),
                    other_task: other["key"].as_str()?.into(),
                    label: link["type"][if outward { "outward" } else { "inward" }]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                    outward,
                })
            })
            .collect(),
    }
}
pub(super) fn receipt(r: i::WriteReceipt) -> Vec<u8> {
    // Bounded durable result preserves a confirmed create even if refresh failed.
    serde_json::to_vec(&serde_json::json!({"task_id":r.task.as_ref().map(|t|&t.reference.id),"title":r.task.as_ref().map(|t|&t.reference.title),"url":r.task.as_ref().map(|t|&t.url),"deleted_task_id":r.deleted_task.as_ref().map(|t|&t.id),"comment_id":r.comment.as_ref().map(|c|&c.id),"deleted_comment_id":r.deleted_comment,"notice":r.notice})).expect("JSON result contains only strings")
}
