//! Canopy's MCP server for chat agents (`canopy-desktop --chat-tools`): `show_html` puts
//! a page in the conversation (the chat draws it from the call), `preview_html` renders
//! one for the agent to check first. JSON-RPC 2.0 over stdio, one message per line.
use super::page::{self, Page};
use base64::Engine as _;
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};

/// Longest line read from the agent.
const MAX_LINE: usize = 4 * 1024 * 1024;
/// Width a preview renders at unless the agent asks for another.
const PREVIEW_WIDTH: f64 = 728.;

/// Serves the agent until its stdin closes.
pub fn serve() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        match reader
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_line(&mut line)
        {
            Ok(0) | Err(_) => return,
            Ok(_) if line.len() > MAX_LINE => {
                // Too long to be a message: drop the rest of it.
                let mut rest = String::new();
                let _ = reader.read_line(&mut rest);
                continue;
            }
            Ok(_) => {}
        }
        let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if let Some(reply) = handle(&message, &preview_from_env) {
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

/// Renders a page for `preview_html`.
type Preview<'a> = dyn Fn(&str, f32) -> Result<super::pages::Preview, String> + 'a;

/// `preview_html` through the Canopy that started this chat.
fn preview_from_env(html: &str, width: f32) -> Result<super::pages::Preview, String> {
    let socket = std::env::var_os(super::pages::SOCKET_ENV)
        .ok_or("Page previews aren't available in this chat.")?;
    let token = std::env::var(super::pages::TOKEN_ENV)
        .map_err(|_| "Page previews aren't available in this chat.")?;
    super::pages::preview(std::path::Path::new(&socket), &token, html, width)
}

/// The answer to one message, `None` for notifications.
fn handle(message: &Value, preview: &Preview) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": message["params"]["protocolVersion"]
                .as_str()
                .unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": page::SERVER, "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Canopy shows pages you write in the chat. Use preview_html to check a page \
                (screenshot, content height, console), then show_html to put it in the conversation.",
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => Ok(call(&message["params"], preview)),
        _ => Err(json!({"code": -32601, "message": format!("Unknown method: {method}")})),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "show_html",
            "description": format!(
                "Show a finished HTML page (chart, table, diagram, before/after mockup) inline in the chat, \
                 where the user can hover and click it. Check it with preview_html first. {}",
                page::PAGE_GUIDE
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short name of the page."},
                    "html": {"type": "string", "description": "A complete, self-contained HTML document (at most 512 KB)."},
                    "height": {"type": "integer", "description": "Expected height in CSS pixels; the page reports its own once it renders."}
                },
                "required": ["title", "html"]
            }
        },
        {
            "name": "preview_html",
            "description": format!(
                "Render an HTML page off screen and get a PNG screenshot, its content height and its console \
                 output (logs, warnings, errors, uncaught exceptions), to check and fix it before show_html. {}",
                page::PAGE_GUIDE
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "A complete, self-contained HTML document."},
                    "width": {"type": "integer", "description": "Viewport width in CSS pixels, 240-1600. Defaults to 728, the chat's column."}
                },
                "required": ["html"]
            }
        }
    ])
}

fn text(text: impl Into<String>, error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}], "isError": error})
}

fn call(params: &Value, preview: &Preview) -> Value {
    let arguments = &params["arguments"];
    match params["name"].as_str().unwrap_or_default() {
        "show_html" => match Page::from_input(arguments) {
            Ok(page) => text(
                format!("Shown to the user in the chat as \"{}\".", page.title),
                false,
            ),
            Err(error) => text(error, true),
        },
        "preview_html" => {
            let html = arguments["html"].as_str().unwrap_or_default();
            if html.trim().is_empty() {
                return text("The page has no HTML.", true);
            }
            if html.len() > page::MAX_BYTES {
                return text("The page is larger than 512 KB.", true);
            }
            let width = arguments["width"]
                .as_f64()
                .unwrap_or(PREVIEW_WIDTH)
                .clamp(240., 1600.) as f32;
            match preview(html, width) {
                Ok(rendered) => {
                    let console = if rendered.console.is_empty() {
                        "(no console output)".to_owned()
                    } else {
                        rendered.console.join("\n")
                    };
                    json!({
                        "content": [
                            {
                                "type": "image",
                                "data": base64::engine::general_purpose::STANDARD.encode(&rendered.png),
                                "mimeType": "image/png"
                            },
                            {
                                "type": "text",
                                "text": format!(
                                    "contentHeight: {:.0}px at width {:.0}px\nconsole:\n{console}",
                                    rendered.height, width
                                )
                            }
                        ],
                        "isError": false
                    })
                }
                Err(error) => text(error, true),
            }
        }
        name => text(format!("Unknown tool: {name}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(html: &str, width: f32) -> Result<super::super::pages::Preview, String> {
        Ok(super::super::pages::Preview {
            png: vec![9],
            height: width / 4.,
            console: if html.contains("bad") {
                vec!["error: ReferenceError: x is not defined".into()]
            } else {
                vec![]
            },
        })
    }

    fn ask(message: Value) -> Value {
        handle(&message, &rendered).expect("a reply")
    }

    #[test]
    fn the_server_describes_its_two_tools() {
        let init = ask(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "canopy");
        let names: Vec<_> =
            ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["name"].as_str().unwrap().to_owned())
                .collect();
        assert_eq!(names, ["show_html", "preview_html"]);
        assert!(
            handle(
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                &rendered
            )
            .is_none()
        );
        assert_eq!(
            ask(json!({"jsonrpc":"2.0","id":3,"method":"nope"}))["error"]["code"],
            -32601
        );
    }

    #[test]
    fn show_checks_the_page_and_preview_returns_image_and_console() {
        let call = |name: &str, arguments: Value| {
            ask(json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":arguments}}))["result"]
                .clone()
        };
        let shown = call("show_html", json!({"title": "Chart", "html": "<p>x</p>"}));
        assert_eq!(shown["isError"], false);
        assert!(
            shown["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Chart")
        );
        assert_eq!(call("show_html", json!({"html": ""}))["isError"], true);
        let preview = call("preview_html", json!({"html": "<p>bad</p>", "width": 800}));
        assert_eq!(preview["content"][0]["type"], "image");
        assert_eq!(preview["content"][0]["data"], "CQ==");
        let report = preview["content"][1]["text"].as_str().unwrap();
        assert!(
            report.contains("contentHeight: 200px") && report.contains("ReferenceError"),
            "{report}"
        );
        assert_eq!(call("preview_html", json!({}))["isError"], true);
    }
}
