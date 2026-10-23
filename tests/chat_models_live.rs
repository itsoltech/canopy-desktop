//! The model picker's background listing against the installed CLIs (DEC-098).
//! Run with `CANOPY_LIVE_CLAUDE=1 CANOPY_LIVE_CODEX=1 cargo test --test chat_models_live -- --ignored`.
use canopy_desktop::chat::{launch::Launch, models, process::Spec, provider::Provider};

fn launch(program: &str, arguments: &[&str]) -> Launch {
    let program = which(program).expect("CLI on PATH");
    Launch {
        spec: Spec {
            program,
            arguments: arguments.iter().map(|a| a.to_string()).collect(),
            cwd: std::env::temp_dir(),
            environment: std::env::vars().collect(),
            secrets: vec![],
        },
        files: None,
    }
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| std::path::Path::new(dir).join(program))
        .find(|path| path.is_file())
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn claude_lists_its_models() {
    if std::env::var_os("CANOPY_LIVE_CLAUDE").is_none() {
        return;
    }
    let list = models::fetch(
        launch(
            "claude",
            &[
                "--output-format",
                "stream-json",
                "--verbose",
                "--input-format",
                "stream-json",
            ],
        ),
        Provider::Claude,
    )
    .expect("models");
    assert!(!list.is_empty(), "Claude listed no models");
}

#[test]
#[ignore = "live Codex; set CANOPY_LIVE_CODEX=1"]
fn codex_lists_its_models() {
    if std::env::var_os("CANOPY_LIVE_CODEX").is_none() {
        return;
    }
    let list = models::fetch(launch("codex", &["app-server"]), Provider::Codex).expect("models");
    assert!(!list.is_empty(), "Codex listed no models");
}
