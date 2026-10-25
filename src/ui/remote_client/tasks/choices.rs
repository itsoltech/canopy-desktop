//! A native-value picker keeps the exact server-provided compound value. In
//! particular cascading Jira IDs must not be reduced to their parent ID.
use super::fields::json;
use super::*;
use gpui_kit::component::{
    combobox::{ComboboxEvent, ComboboxState},
    select::SearchableVec,
};
use serde_json::Value as Json;
pub(super) struct NativeChanged;
pub(super) struct NativeChoices {
    pub(super) field: p::TaskField,
    picker: Entity<ComboboxState<SearchableVec<SelectOption>>>,
    selected: Vec<String>,
    baseline: Json,
    values: Vec<(String, Json)>,
    enabled: bool,
    changed: bool,
    _events: Subscription,
}
impl EventEmitter<NativeChanged> for NativeChoices {}
fn contains(full: &Json, part: &Json) -> bool {
    match (full, part) {
        (Json::Object(full), Json::Object(part)) => part
            .iter()
            .all(|(k, v)| full.get(k).is_some_and(|f| contains(f, v))),
        _ => full == part,
    }
}
impl NativeChoices {
    pub(super) fn new(
        field: p::TaskField,
        current: Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut values: Vec<_> = field
            .choices
            .iter()
            .map(|c| (c.id.clone(), json(c.value.as_ref())))
            .collect();
        let mut options: Vec<_> = field
            .choices
            .iter()
            .map(|c| SelectOption::new(c.id.clone(), c.label.clone()))
            .collect();
        let initial = if field.multiple {
            current.as_array().cloned().unwrap_or_default()
        } else if current.is_null() {
            vec![]
        } else {
            vec![current.clone()]
        };
        let selected: Vec<String> = initial
            .into_iter()
            .enumerate()
            .map(|(ix, v)| {
                if let Some((key, _)) = values.iter().find(|(_, c)| contains(&v, c)) {
                    return key.clone();
                }
                let key = format!("retained:{ix}");
                options.push(SelectOption::new(
                    key.clone(),
                    "Current value (not in this page)",
                ));
                values.push((key.clone(), v));
                key
            })
            .collect();
        let picker = cx.new(|cx| {
            ComboboxState::new(SearchableVec::new(options), vec![], window, cx)
                .multiple(field.multiple)
                .searchable(true)
        });
        picker.update(cx, |s, cx| {
            s.set_selected_values(
                &selected
                    .iter()
                    .cloned()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
                window,
                cx,
            )
        });
        let events = cx.subscribe(
            &picker,
            |s, _, event: &ComboboxEvent<SearchableVec<SelectOption>>, cx| {
                if let ComboboxEvent::Change(ids) = event {
                    s.changed = true;
                    s.selected = ids.iter().map(ToString::to_string).collect();
                    cx.emit(NativeChanged);
                    cx.notify();
                }
            },
        );
        Self {
            field,
            picker,
            selected,
            baseline: current,
            values,
            enabled: true,
            changed: false,
            _events: events,
        }
    }
    pub(super) fn value(&self) -> Json {
        if !self.changed {
            return self.baseline.clone();
        }
        let values: Vec<_> = self
            .selected
            .iter()
            .filter_map(|key| {
                self.values
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
            })
            .collect();
        if self.field.multiple {
            Json::Array(values)
        } else {
            values.into_iter().next().unwrap_or(Json::Null)
        }
    }
    pub(super) fn dirty(&self) -> bool {
        self.value() != self.baseline
    }
    pub(super) fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
    pub(super) fn restore(&mut self, value: &Json, window: &mut Window, cx: &mut Context<Self>) {
        let values = if self.field.multiple {
            value.as_array().cloned().unwrap_or_default()
        } else {
            vec![value.clone()]
        };
        let keys: Vec<_> = values
            .iter()
            .filter_map(|v| {
                self.values
                    .iter()
                    .find(|(_, c)| contains(v, c))
                    .map(|(k, _)| k.clone())
            })
            .collect();
        self.changed = true;
        self.selected = keys.clone();
        self.picker.update(cx, |s, cx| {
            s.set_selected_values(
                &keys.into_iter().map(SharedString::from).collect::<Vec<_>>(),
                window,
                cx,
            )
        });
        cx.notify();
    }
}
impl Render for NativeChoices {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        form_field(
            format!(
                "{}{}",
                self.field.name,
                if self.field.required { " *" } else { "" }
            ),
            "",
            combobox(&self.picker).disabled(!self.enabled),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn cascading_current_value_matches_child_not_only_parent() {
        let current =
            serde_json::json!({"id":"001","value":"Parent","child":{"id":"02","value":"Child"}});
        assert!(contains(
            &current,
            &serde_json::json!({"id":"001","child":{"id":"02"}})
        ));
        assert!(!contains(
            &current,
            &serde_json::json!({"id":"001","child":{"id":"03"}})
        ));
        assert!(!contains(
            &current,
            &serde_json::json!({"id":1,"child":{"id":"02"}})
        ));
    }
}
