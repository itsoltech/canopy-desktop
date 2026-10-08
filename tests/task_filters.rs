use canopy_desktop::{
    integrations::{
        attachments::{PREVIEW_LIMIT, PreviewFile, preview_name},
        filters::*,
        *,
    },
    settings::{Access, SettingsClient},
};
use serde_json::json;
fn project(key: &str) -> ProjectTarget {
    ProjectTarget::jira("https://team.atlassian.net", key).unwrap()
}
fn custom(name: &str, expression: &str) -> TaskFilter {
    TaskFilter {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        provider: Provider::Jira,
        expression: expression.into(),
    }
}
#[test]
fn builtins_are_dynamic_and_provider_scoped() {
    let config = Config::default();
    assert_eq!(config.filters_for(Provider::Jira).len(), 7);
    assert!(config.filters_for(Provider::Github).is_empty());
    assert_eq!(
        config.selected_filter(&project("GAKKO")).unwrap().id,
        DEFAULT_FILTER
    );
    assert_eq!(
        builtin("jira:sprint-unassigned").unwrap().expression,
        "sprint in openSprints() AND assignee is EMPTY"
    );
    for (_, _, expression) in BUILTINS {
        validate_expression(expression).unwrap();
    }
}
#[test]
fn filters_validate_composition_without_rewriting_jql() {
    for value in [
        "",
        "assignee is EMPTY",
        "(labels = urgent OR priority = High) AND sprint in openSprints()",
        "summary ~ '\"quoted\" phrase'",
        "summary ~ \"ORDER BY\"",
    ] {
        validate_expression(value).unwrap();
    }
    for value in [
        "assignee is EMPTY) OR project=OTHER",
        "(status = Open",
        "summary ~ \"unclosed",
        "status=Open ORDER BY created",
        "status=Open; project=OTHER",
    ] {
        assert!(validate_expression(value).is_err(), "{value}");
    }
    assert!(validate_expression(&"x".repeat(4097)).is_err());
}
#[test]
fn selected_filters_are_independent_between_projects_and_sites() {
    let filter = custom("Needs owner", "assignee is EMPTY");
    let mut c = Config::default().save_filter(filter.clone()).unwrap();
    c = c.select_filter(&project("GAKKO"), &filter.id).unwrap();
    c = c.select_filter(&project("ISSUE"), "jira:sprint").unwrap();
    assert_eq!(c.selected_filter(&project("GAKKO")).unwrap().id, filter.id);
    assert_eq!(
        c.selected_filter(&project("ISSUE")).unwrap().id,
        "jira:sprint"
    );
    let other = ProjectTarget::jira("https://another.atlassian.net", "GAKKO").unwrap();
    assert_eq!(c.selected_filter(&other).unwrap().id, DEFAULT_FILTER);
    assert!(
        c.select_filter(&ProjectTarget::github("owner/repo").unwrap(), &filter.id)
            .is_err()
    );
}
#[test]
fn editing_keeps_id_and_deleting_resets_only_affected_projects() {
    let mut f = custom("Mine", "assignee=currentUser()");
    let c = Config::default()
        .save_filter(f.clone())
        .unwrap()
        .select_filter(&project("GAKKO"), &f.id)
        .unwrap()
        .select_filter(&project("ISSUE"), "jira:sprint")
        .unwrap();
    f.name = "Assigned to me".into();
    f.expression = "assignee=currentUser() AND statusCategory != Done".into();
    let c = c.save_filter(f.clone()).unwrap();
    assert_eq!(c.selected_filter(&project("GAKKO")).unwrap(), f);
    assert!(
        c.save_filter(custom("Assigned to me", "labels=urgent"))
            .is_err()
    );
    let c = c.remove_filter(&f.id);
    assert!(c.valid());
    assert_eq!(
        c.selected_filter(&project("GAKKO")).unwrap().id,
        DEFAULT_FILTER
    );
    assert_eq!(
        c.selected_filter(&project("ISSUE")).unwrap().id,
        "jira:sprint"
    );
}
#[test]
fn filters_and_project_mapping_survive_database_restart() {
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("filters.db");
        let db = SettingsClient::create(&path).await.unwrap();
        let f = custom(
            "Unassigned this sprint",
            "assignee is EMPTY AND sprint in openSprints()",
        );
        let mut c = Config::default()
            .save_filter(f.clone())
            .unwrap()
            .select_filter(&project("GAKKO"), &f.id)
            .unwrap();
        c.overrides
            .insert(dir.path().join("repo"), project("ISSUE"));
        db.save_integrations(c.clone()).await.unwrap();
        db.shutdown().await.unwrap();
        let db = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(db.load_integrations().await.unwrap(), c);
        db.shutdown().await.unwrap();
    });
}
#[test]
fn older_configuration_gets_builtins_without_inventing_custom_data() {
    let c: Config =
        serde_json::from_value(json!({"accounts":[],"overrides":{},"links":{}})).unwrap();
    assert!(c.valid());
    assert!(c.task_filters.is_empty());
    assert!(c.filter_selections.is_empty());
    assert_eq!(c.filters_for(Provider::Jira).len(), 7);
}
#[test]
fn preview_files_are_private_read_only_and_removed_with_the_owner() {
    let file = PreviewFile::create("../../private.txt", b"attachment contents").unwrap();
    let path = file.path().to_owned();
    let parent = path.parent().unwrap().to_owned();
    assert_eq!(path.file_name().unwrap(), "private.txt");
    assert_eq!(std::fs::read(&path).unwrap(), b"attachment contents");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        assert_eq!(
            std::fs::metadata(&parent).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    drop(file);
    assert!(!path.exists());
    assert!(!parent.exists());
}
#[test]
fn preview_save_is_explicit_and_does_not_mutate_the_cached_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("saved.txt");
    std::fs::write(&out, b"old").unwrap();
    let file = PreviewFile::create("note.txt", b"new").unwrap();
    file.save_to(&out).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), b"new");
    assert_eq!(std::fs::read(file.path()).unwrap(), b"new");
}
#[test]
fn preview_names_keep_type_without_escaping_the_private_directory() {
    assert_eq!(preview_name(".."), "attachment");
    assert_eq!(preview_name("a\\b\\note.pdf"), "note.pdf");
    assert_eq!(preview_name("a:b\n.pdf"), "a_b_.pdf");
    assert_eq!(preview_name("CON.txt"), "_CON.txt");
    assert_eq!(preview_name("LPT1.log"), "_LPT1.log");
    assert_eq!(preview_name("CON .txt"), "_CON .txt");
    assert_eq!(preview_name("nul. "), "_nul");
    assert_eq!(preview_name("report?.pdf"), "report_.pdf");
    assert!(preview_name(&format!("{}.pdf", "a".repeat(300))).ends_with(".pdf"));
    let unicode_name = preview_name(&format!("{}.png", "🦀".repeat(200)));
    assert!(unicode_name.len() <= 180);
    assert!(unicode_name.ends_with(".png"));
    assert!(PreviewFile::create("huge.bin", &vec![0; PREVIEW_LIMIT + 1]).is_err());
}
