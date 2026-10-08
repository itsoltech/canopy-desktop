//! File-type icons (Material Icon Theme, MIT — `assets/icons/files/`), shown as colored
//! images next to file names.
use std::path::Path;

const ICONS: &[(&str, &[u8])] = &[
    ("c", include_bytes!("../../assets/icons/files/c.svg")),
    (
        "console",
        include_bytes!("../../assets/icons/files/console.svg"),
    ),
    ("cpp", include_bytes!("../../assets/icons/files/cpp.svg")),
    (
        "csharp",
        include_bytes!("../../assets/icons/files/csharp.svg"),
    ),
    ("css", include_bytes!("../../assets/icons/files/css.svg")),
    ("dart", include_bytes!("../../assets/icons/files/dart.svg")),
    (
        "database",
        include_bytes!("../../assets/icons/files/database.svg"),
    ),
    (
        "docker",
        include_bytes!("../../assets/icons/files/docker.svg"),
    ),
    (
        "document",
        include_bytes!("../../assets/icons/files/document.svg"),
    ),
    (
        "elixir",
        include_bytes!("../../assets/icons/files/elixir.svg"),
    ),
    ("git", include_bytes!("../../assets/icons/files/git.svg")),
    ("go", include_bytes!("../../assets/icons/files/go.svg")),
    ("h", include_bytes!("../../assets/icons/files/h.svg")),
    (
        "haskell",
        include_bytes!("../../assets/icons/files/haskell.svg"),
    ),
    ("html", include_bytes!("../../assets/icons/files/html.svg")),
    (
        "image",
        include_bytes!("../../assets/icons/files/image.svg"),
    ),
    ("java", include_bytes!("../../assets/icons/files/java.svg")),
    (
        "javascript",
        include_bytes!("../../assets/icons/files/javascript.svg"),
    ),
    ("json", include_bytes!("../../assets/icons/files/json.svg")),
    (
        "kotlin",
        include_bytes!("../../assets/icons/files/kotlin.svg"),
    ),
    ("lock", include_bytes!("../../assets/icons/files/lock.svg")),
    ("log", include_bytes!("../../assets/icons/files/log.svg")),
    ("lua", include_bytes!("../../assets/icons/files/lua.svg")),
    (
        "markdown",
        include_bytes!("../../assets/icons/files/markdown.svg"),
    ),
    ("pdf", include_bytes!("../../assets/icons/files/pdf.svg")),
    ("php", include_bytes!("../../assets/icons/files/php.svg")),
    (
        "python",
        include_bytes!("../../assets/icons/files/python.svg"),
    ),
    (
        "react",
        include_bytes!("../../assets/icons/files/react.svg"),
    ),
    (
        "react_ts",
        include_bytes!("../../assets/icons/files/react_ts.svg"),
    ),
    ("ruby", include_bytes!("../../assets/icons/files/ruby.svg")),
    ("rust", include_bytes!("../../assets/icons/files/rust.svg")),
    ("sass", include_bytes!("../../assets/icons/files/sass.svg")),
    (
        "scala",
        include_bytes!("../../assets/icons/files/scala.svg"),
    ),
    (
        "settings",
        include_bytes!("../../assets/icons/files/settings.svg"),
    ),
    (
        "svelte",
        include_bytes!("../../assets/icons/files/svelte.svg"),
    ),
    ("svg", include_bytes!("../../assets/icons/files/svg.svg")),
    (
        "swift",
        include_bytes!("../../assets/icons/files/swift.svg"),
    ),
    ("toml", include_bytes!("../../assets/icons/files/toml.svg")),
    ("tune", include_bytes!("../../assets/icons/files/tune.svg")),
    (
        "typescript",
        include_bytes!("../../assets/icons/files/typescript.svg"),
    ),
    ("vue", include_bytes!("../../assets/icons/files/vue.svg")),
    ("xml", include_bytes!("../../assets/icons/files/xml.svg")),
    ("yaml", include_bytes!("../../assets/icons/files/yaml.svg")),
    ("zip", include_bytes!("../../assets/icons/files/zip.svg")),
];

/// Bytes of `canopy/files/<name>.svg`, for the app's asset source.
pub fn load(path: &str) -> Option<&'static [u8]> {
    let name = path.strip_prefix("canopy/files/")?.strip_suffix(".svg")?;
    ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, bytes)| *bytes)
}

/// The asset path of the icon for `path`, by file name or extension.
pub fn for_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let by_name = match name.as_str() {
        "dockerfile" | "containerfile" => Some("docker"),
        ".gitignore" | ".gitattributes" | ".gitmodules" => Some("git"),
        "cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "bun.lockb" => {
            Some("lock")
        }
        _ if name.starts_with(".env") => Some("tune"),
        _ => None,
    };
    let icon = by_name.or_else(|| {
        Some(
            match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
                "rs" => "rust",
                "ts" | "mts" | "cts" => "typescript",
                "tsx" => "react_ts",
                "jsx" => "react",
                "js" | "mjs" | "cjs" => "javascript",
                "py" | "pyi" => "python",
                "go" => "go",
                "java" => "java",
                "kt" | "kts" => "kotlin",
                "swift" => "swift",
                "c" => "c",
                "h" | "hpp" | "hh" => "h",
                "cpp" | "cc" | "cxx" => "cpp",
                "cs" => "csharp",
                "rb" => "ruby",
                "php" => "php",
                "html" | "htm" => "html",
                "css" => "css",
                "scss" | "sass" => "sass",
                "json" | "jsonc" | "json5" => "json",
                "yaml" | "yml" => "yaml",
                "toml" => "toml",
                "md" | "mdx" | "markdown" => "markdown",
                "svelte" => "svelte",
                "vue" => "vue",
                "sh" | "bash" | "zsh" | "fish" => "console",
                "lock" => "lock",
                "ini" | "cfg" | "conf" | "plist" => "settings",
                "xml" => "xml",
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" => "image",
                "svg" => "svg",
                "pdf" => "pdf",
                "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" => "zip",
                "sql" | "sqlite" | "db" => "database",
                "lua" => "lua",
                "dart" => "dart",
                "ex" | "exs" => "elixir",
                "hs" => "haskell",
                "scala" | "sc" => "scala",
                "txt" | "rtf" => "document",
                "log" => "log",
                _ => return None,
            },
        )
    })?;
    Some(format!("canopy/files/{icon}.svg"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn known_files_have_icons_that_load() {
        for (file, icon) in [
            ("src/main.rs", "rust"),
            ("App.tsx", "react_ts"),
            ("Dockerfile", "docker"),
            (".env.local", "tune"),
            ("notes.MD", "markdown"),
        ] {
            let path = for_path(Path::new(file)).unwrap();
            assert_eq!(path, format!("canopy/files/{icon}.svg"));
            assert!(load(&path).is_some(), "{path}");
        }
        assert_eq!(for_path(Path::new("x.unknown")), None);
    }
}
