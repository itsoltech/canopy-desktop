//! Typed transport adapters for the existing provider form controls.
use super::*;
use crate::ui::{jira::field::FieldInput, youtrack::form::YoutrackFieldInput};
use canopy_desktop::integrations::{
    jira::JiraField,
    youtrack::{YoutrackField, YoutrackFieldKind},
};
use serde_json::{Value as Json, json};

pub(super) fn json(value: Option<&prost_types::Value>) -> Json {
    use prost_types::value::Kind;
    match value.and_then(|v| v.kind.as_ref()) {
        None | Some(Kind::NullValue(_)) => Json::Null,
        Some(Kind::BoolValue(v)) => json!(v),
        Some(Kind::NumberValue(v))
            if v.is_finite() && v.fract() == 0. && v.abs() <= 9_007_199_254_740_991. =>
        {
            json!(*v as i64)
        }
        Some(Kind::NumberValue(v)) => json!(v),
        Some(Kind::StringValue(v)) => json!(v),
        Some(Kind::ListValue(v)) => Json::Array(v.values.iter().map(|v| json(Some(v))).collect()),
        Some(Kind::StructValue(v)) => object(Some(v)),
    }
}
pub(super) fn object(value: Option<&prost_types::Struct>) -> Json {
    Json::Object(
        value
            .map(|v| {
                v.fields
                    .iter()
                    .map(|(k, v)| (k.clone(), json(Some(v))))
                    .collect()
            })
            .unwrap_or_default(),
    )
}
pub(super) fn proto(value: Json) -> prost_types::Value {
    use prost_types::value::Kind;
    prost_types::Value {
        kind: Some(match value {
            Json::Null => Kind::NullValue(0),
            Json::Bool(v) => Kind::BoolValue(v),
            Json::Number(v) => Kind::NumberValue(v.as_f64().unwrap_or_default()),
            Json::String(v) => Kind::StringValue(v),
            Json::Array(v) => Kind::ListValue(prost_types::ListValue {
                values: v.into_iter().map(proto).collect(),
            }),
            Json::Object(v) => Kind::StructValue(prost_types::Struct {
                fields: v.into_iter().map(|(k, v)| (k, proto(v))).collect(),
            }),
        }),
    }
}
pub(super) enum FieldView {
    Choices(Entity<super::choices::NativeChoices>),
    Jira(Entity<FieldInput>),
    Youtrack(Entity<YoutrackFieldInput>),
    ReadOnly(p::TaskField),
}
impl FieldView {
    pub(super) fn new(
        field: p::TaskField,
        provider: &str,
        create: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        if field.read_only {
            return Self::ReadOnly(field);
        }
        let current = if create {
            json(field.default_value.as_ref())
        } else {
            json(field.current_value.as_ref())
        };
        if provider == "jira" && !field.choices.is_empty() {
            return Self::Choices(
                cx.new(|cx| super::choices::NativeChoices::new(field, current, window, cx)),
            );
        }
        if provider == "youtrack" {
            let schema = object(field.provider_schema.as_ref());
            let kind = match field.kind.as_str() {
                "string" => YoutrackFieldKind::String,
                "object" | "text" => YoutrackFieldKind::Text,
                "date" => YoutrackFieldKind::Date,
                "integer" => YoutrackFieldKind::Integer,
                "float" => YoutrackFieldKind::Float,
                "boolean" => YoutrackFieldKind::Boolean,
                "enum" => YoutrackFieldKind::Enum,
                "state" => YoutrackFieldKind::State,
                "statemachine" => YoutrackFieldKind::StateMachine,
                "user" => YoutrackFieldKind::User,
                "group" => YoutrackFieldKind::Group,
                "version" => YoutrackFieldKind::Version,
                "build" => YoutrackFieldKind::Build,
                "ownedfield" => YoutrackFieldKind::OwnedField,
                "period" => YoutrackFieldKind::Period,
                _ => YoutrackFieldKind::Unsupported,
            };
            let f = YoutrackField {
                id: field.id,
                name: field.name,
                kind,
                field_type: schema["$type"].as_str().unwrap_or_default().into(),
                field_type_id: schema["fieldType"].as_str().unwrap_or_default().into(),
                bundle_id: schema["bundleId"].as_str().map(str::to_owned),
                required: field.required,
                read_only: field.read_only,
                multi_value: field.multiple,
                value: current,
                default: json(field.default_value.as_ref()),
                allowed: field
                    .choices
                    .into_iter()
                    .map(|c| json(c.value.as_ref()))
                    .collect(),
                events: vec![],
            };
            Self::Youtrack(cx.new(|cx| YoutrackFieldInput::new(f, create, window, cx)))
        } else {
            let mut schema = object(field.provider_schema.as_ref());
            if schema.as_object().is_none_or(|s| s.is_empty()) {
                schema = json!({"type":field.kind});
            }
            let allowed = field
                .choices
                .into_iter()
                .map(|c| {
                    let mut v = json(c.value.as_ref());
                    if let Some(o) = v.as_object_mut() {
                        o.entry("name").or_insert(json!(c.label));
                    }
                    v
                })
                .collect();
            let f = JiraField {
                id: field.id,
                name: field.name,
                required: field.required,
                schema,
                allowed,
                default: json(field.default_value.as_ref()),
            };
            Self::Jira(cx.new(|cx| FieldInput::new(f, current, String::new(), window, cx)))
        }
    }
    pub(super) fn id(&self, cx: &App) -> String {
        match self {
            Self::Choices(v) => v.read(cx).field.id.clone(),
            Self::Jira(v) => v.read(cx).field.id.clone(),
            Self::Youtrack(v) => v.read(cx).id().into(),
            Self::ReadOnly(f) => f.id.clone(),
        }
    }
    pub(super) fn dirty(&self, cx: &App) -> bool {
        match self {
            Self::Choices(v) => v.read(cx).dirty(),
            Self::Jira(v) => v.read(cx).dirty(cx),
            Self::Youtrack(v) => v.read(cx).dirty(cx),
            Self::ReadOnly(_) => false,
        }
    }
    pub(super) fn value(&self, cx: &App) -> Result<Json, String> {
        match self {
            Self::Choices(v) => Ok(v.read(cx).value()),
            Self::Jira(v) => v.read(cx).value(cx),
            Self::Youtrack(v) => v.read(cx).value(cx),
            Self::ReadOnly(f) => Ok(json(f.current_value.as_ref())),
        }
    }
    pub(super) fn enable(&self, enabled: bool, cx: &mut App) {
        match self {
            Self::Choices(v) => v.update(cx, |v, cx| v.enable(enabled, cx)),
            Self::Jira(v) => v.update(cx, |v, cx| v.enable(enabled, cx)),
            Self::Youtrack(v) => v.update(cx, |v, cx| v.enable(enabled, cx)),
            _ => {}
        }
    }
    pub(super) fn render(&self) -> AnyElement {
        match self {
            Self::Choices(v) => v.clone().into_any_element(),
            Self::Jira(v) => v.clone().into_any_element(),
            Self::Youtrack(v) => v.clone().into_any_element(),
            Self::ReadOnly(f) => form_field(
                f.name.clone(),
                f.read_only_reason
                    .clone()
                    .unwrap_or_else(|| "Read only".into()),
                div().text_size(px(12.)).text_color(t::secondary()).child(
                    super::adapter::display(&json(f.current_value.as_ref()))
                        .unwrap_or_else(|| "—".into()),
                ),
            )
            .into_any_element(),
        }
    }
}
impl FieldView {
    pub(super) fn draft(&self, cx: &App) -> Json {
        match self {
            Self::Choices(v) => v.read(cx).value(),
            Self::Jira(v) => v.read(cx).draft(cx),
            Self::Youtrack(v) => v.read(cx).draft(cx),
            Self::ReadOnly(f) => json(f.current_value.as_ref()),
        }
    }
    pub(super) fn restore(&self, value: &Json, window: &mut Window, cx: &mut App) {
        match self {
            Self::Choices(v) => v.update(cx, |v, cx| v.restore(value, window, cx)),
            Self::Jira(v) => v.update(cx, |v, cx| v.restore(value, window, cx)),
            Self::Youtrack(v) => v.update(cx, |v, cx| v.restore(value, window, cx)),
            _ => {}
        }
    }
    pub(super) fn subscribe(&self, cx: &mut Context<TasksView>) -> Option<Subscription> {
        match self {
            Self::Jira(v) => Some(
                cx.subscribe(v, |s, _, _: &crate::ui::jira::field::FieldChanged, cx| {
                    s.persist_draft(cx)
                }),
            ),
            Self::Youtrack(v) => Some(cx.subscribe(
                v,
                |s, _, _: &crate::ui::youtrack::form::YoutrackFieldChanged, cx| s.persist_draft(cx),
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn native_document_roundtrip_preserves_adf_version_and_opaque_ids() {
        let document = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"001:user:opaque","text":"@User"}},{"type":"text","text":"link","marks":[{"type":"link","attrs":{"href":"https://example.invalid/issue/001"}}]}]}]});
        let wire = proto(document.clone());
        let decoded = json(Some(&wire));
        assert_eq!(decoded["version"].as_u64(), Some(1));
        assert_eq!(decoded, document);
        let editing = canopy_desktop::integrations::jira::adf::editing_text(&decoded);
        let restored =
            canopy_desktop::integrations::jira::adf::from_editing_text(&editing, &decoded).unwrap();
        assert!(restored.to_string().contains("001:user:opaque"));
    }
    #[test]
    fn native_values_keep_fractional_numbers_and_compound_choices() {
        let original = json!({"duration":1.25,"choice":{"id":"0007","child":{"id":"08:opaque"}},"unset":null,"multiple":[{"accountId":"001"}]});
        assert_eq!(json(Some(&proto(original.clone()))), original);
    }
}
