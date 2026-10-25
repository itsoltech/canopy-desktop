//! Pages agents show in the chat (DEC-076..078): the `show_html` / `preview_html` tools
//! Canopy offers over MCP, what a call asks for, and the HTML made ready to render.
use serde_json::Value;
use std::path::Path;

/// The MCP server's name; its tools reach the transcript as `mcp__canopy__<tool>`.
pub const SERVER: &str = "canopy";
pub const SHOW_TOOL: &str = "mcp__canopy__show_html";
pub const PREVIEW_TOOL: &str = "mcp__canopy__preview_html";
/// Largest page accepted, in bytes.
pub const MAX_BYTES: usize = 512 * 1024;
/// Tallest a page is shown in the conversation; longer content scrolls inside it.
pub const MAX_HEIGHT: f32 = 2000.;
/// Local images inlined into a page are at most this large.
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// A page to show: what the agent asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub title: String,
    pub html: String,
    /// The agent's guess of its height (the page reports its own once it renders).
    pub height: Option<f32>,
}

impl Page {
    /// The page a `show_html` call asks for.
    pub fn from_input(input: &Value) -> Result<Self, String> {
        let html = input["html"]
            .as_str()
            .filter(|html| !html.trim().is_empty())
            .ok_or("The page has no HTML.")?;
        if html.len() > MAX_BYTES {
            return Err(format!(
                "The page is {} KB; pages are limited to {} KB.",
                html.len() / 1024,
                MAX_BYTES / 1024
            ));
        }
        let title: String = input["title"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        Ok(Self {
            title: if title.trim().is_empty() {
                "Page".into()
            } else {
                title.trim().to_owned()
            },
            html: html.to_owned(),
            height: input["height"]
                .as_f64()
                .filter(|h| *h >= 40.)
                .map(|h| (h as f32).min(MAX_HEIGHT)),
        })
    }
}

/// `html` with images it points to by absolute path (`src="/abs/a.png"`, CSS
/// `url(/abs/a.png)`, or a JS string) put in as data URIs: the page itself reads no
/// files. Missing, oversized or non-image paths stay as they are.
pub fn inline_images(html: &str) -> String {
    inline_images_if(html, &|path| {
        Path::new(path)
            .is_absolute()
            .then(|| std::fs::canonicalize(path).ok())
            .flatten()
    })
}
/// Remote page resources are limited to the authorized chat folder. The page
/// receives bytes, never authority to read paths through its client renderer.
pub fn inline_images_scoped(html: &str, root: &Path) -> String {
    let root = std::fs::canonicalize(root).ok();
    inline_images_if(html, &|path| {
        let root = root.as_ref()?;
        let path = Path::new(path);
        let path = std::fs::canonicalize(if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        })
        .ok()?;
        path.starts_with(root).then_some(path)
    })
}
/// Bound the complete result before allocating image/base64 buffers.
pub const MAX_INLINE_BYTES: usize = 32 * 1024 * 1024;
fn inline_images_if(html: &str, resolve: &dyn Fn(&str) -> Option<std::path::PathBuf>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(['"', '\'', '(']) {
        let (before, from) = rest.split_at(at + 1);
        out.push_str(before);
        let quote = before.chars().last().unwrap_or('"');
        let close = if quote == '(' { ')' } else { quote };
        let end = from.find(close).unwrap_or(0);
        let candidate = from[..end].trim().trim_matches(['"', '\'']);
        let budget = MAX_INLINE_BYTES.saturating_sub(out.len().saturating_add(from.len()));
        match (end > 0)
            .then(|| data_uri(candidate, budget, resolve))
            .flatten()
        {
            Some(uri) => {
                out.push_str(&uri);
                rest = &from[end..];
            }
            None => rest = from,
        }
    }
    out.push_str(rest);
    out
}

fn data_uri(
    path: &str,
    budget: usize,
    resolve: &dyn Fn(&str) -> Option<std::path::PathBuf>,
) -> Option<String> {
    let path = path.strip_prefix("file://").unwrap_or(path);
    if path.len() > 4096 || path.contains(['\n', '<', '>']) || path.contains("://") {
        return None;
    }
    let media = match Path::new(path)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    let path = resolve(path)?;
    if !path.is_absolute() {
        return None;
    }
    // Open the canonical authorized path, not the untrusted symlink spelling.
    if !std::fs::metadata(&path).ok()?.is_file() {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // A file replaced by a FIFO or final symlink must not block shutdown or
        // redirect this already scoped read.
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let max = MAX_IMAGE_BYTES.min((budget.saturating_sub(64) / 4 * 3) as u64);
    if metadata.len() > max {
        return None;
    }
    use std::io::Read as _;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > max {
        return None;
    }
    use base64::Engine as _;
    Some(format!(
        "data:{media};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// What the agent is told about how to write pages (the tools' descriptions).
pub const PAGE_GUIDE: &str = "Write one self-contained HTML document with inline <style> and <script>; \
remote http(s) resources (e.g. a chart library from a CDN) load as-is, and local images given by absolute \
path (src=\"/abs/shot.png\", CSS url(/abs/bg.webp)) are inlined. The page sits in the chat on the \
conversation's background: leave html and body without a background, use a fluid width with no outer \
padding, card or title banner, and let content set the height (no 100vh). Canopy's theme is available as \
CSS variables on :root: --background, --foreground, --muted, --muted-foreground, --card, \
--card-foreground, --border, --accent, --red, --green, --yellow, --radius, --font-sans, --font-mono. \
Links open in the user's browser.";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn calls_become_pages_within_limits() {
        let page = Page::from_input(
            &json!({"title": "Before\u{7} / after", "html": "<p>x</p>", "height": 5000}),
        )
        .unwrap();
        assert_eq!(page.title, "Before / after");
        assert_eq!(page.height, Some(MAX_HEIGHT));
        assert_eq!(
            Page::from_input(&json!({"html": "<b>a</b>"}))
                .unwrap()
                .title,
            "Page"
        );
        assert!(Page::from_input(&json!({"html": "  "})).is_err());
        assert!(
            Page::from_input(&json!({"html": "x".repeat(MAX_BYTES + 1)}))
                .unwrap_err()
                .contains("limited")
        );
    }

    #[test]
    fn remote_images_stay_in_folder_and_total_budget() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let image = dir.path().join("ok.png");
        let secret = outside.path().join("secret.png");
        std::fs::write(&image, [1, 2, 3]).unwrap();
        std::fs::write(&secret, [4, 5, 6]).unwrap();
        let html = format!(r#"<img src="ok.png"><img src="{}">"#, secret.display());
        let result = inline_images_scoped(&html, dir.path());
        assert!(result.contains("data:image/png;base64,AQID"));
        assert!(!result.contains("BAUG"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, dir.path().join("escape.png")).unwrap();
            assert!(
                !inline_images_scoped(r#"<img src="escape.png">"#, dir.path()).contains("data:")
            );
        }
        let large = dir.path().join("large.png");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES)
            .unwrap();
        let html = r#"<img src="large.png">"#.repeat(4);
        let result = inline_images_scoped(&html, dir.path());
        assert!(result.len() <= MAX_INLINE_BYTES);
        assert_eq!(result.matches("data:image/png").count(), 2);
    }

    #[test]
    fn local_images_go_in_as_data() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("shot.png");
        std::fs::write(&image, [137, 80, 78, 71]).unwrap();
        let path = image.to_str().unwrap();
        let html = format!(
            r#"<img src="{path}"><div style="background:url({path})"></div><img src="/nope/x.png"><a href="https://x.dev">x</a>"#
        );
        let out = inline_images(&html);
        assert_eq!(
            out.matches("data:image/png;base64,iVBORw==").count(),
            2,
            "{out}"
        );
        assert!(out.contains(r#"src="/nope/x.png""#) && out.contains("https://x.dev"));
        assert_eq!(
            inline_images("no paths (here) 'or' \"there\""),
            "no paths (here) 'or' \"there\""
        );
    }
}
