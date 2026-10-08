use super::*;
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpRequestExt, RedirectPolicy, Request};
use serde_json::Value;
use std::time::Duration;

const JSON_LIMIT: usize = 8 * 1024 * 1024;
const ATTACHMENT_LIMIT: usize = 25 * 1024 * 1024;

fn valid_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains("..")
        && !path.contains('#')
        && !path.bytes().any(|b| b.is_ascii_control())
}

fn redacted(text: String, token: &str) -> String {
    text.replace(token, "[redacted]")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .take(1800)
        .collect()
}

fn error_text(bytes: &[u8], status: u16, token: &str) -> String {
    let value: Value = serde_json::from_slice(bytes).unwrap_or_default();
    let detail = [
        value["error"].as_str(),
        value["error_description"].as_str(),
        value["message"].as_str(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(": ");
    if detail.is_empty() {
        format!("YouTrack request failed (HTTP {status}).")
    } else {
        format!("YouTrack request failed: {}", redacted(detail, token))
    }
}

impl Youtrack {
    pub(super) async fn get(&self, path: &str) -> Result<Value, String> {
        self.request(method::GET, path, None, "application/json", false)
            .await
            .map_err(|e| e.to_string())
    }

    pub(super) async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        writing: bool,
    ) -> Result<Value, WriteError> {
        self.request(
            method,
            path,
            body.map(|v| v.to_string().into_bytes()),
            "application/json",
            writing,
        )
        .await
    }

    pub(super) async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
        mime: &str,
        writing: bool,
    ) -> Result<Value, WriteError> {
        let relative = path.strip_prefix("/api").unwrap_or(path);
        if !valid_path(relative) {
            return Err("Invalid YouTrack API path.".into());
        }
        let mut authorization =
            gpui_kit::http_client::http::HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|_| "Could not prepare YouTrack credentials.")?;
        authorization.set_sensitive(true);
        let request = Request::builder()
            .method(method)
            .uri(format!("{}{}", self.api, relative))
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("Content-Type", mime)
            .header("User-Agent", "Canopy-Desktop")
            .follow_redirects(RedirectPolicy::NoFollow)
            .timeout(if mime.starts_with("multipart/") {
                Duration::from_secs(60)
            } else {
                Duration::from_secs(30)
            })
            .body(body.map(AsyncBody::from).unwrap_or_else(AsyncBody::empty))
            .map_err(|_| "Could not prepare YouTrack request.")?;
        let uncertain = || {
            WriteError::Uncertain(
                "Could not confirm the YouTrack change. It may already be saved; refresh the task before retrying."
                    .into(),
            )
        };
        let response = self.http.send(request).await.map_err(|_| {
            if writing {
                uncertain()
            } else {
                WriteError::Rejected(
                    "Could not reach YouTrack. Check the service address and connection.".into(),
                )
            }
        })?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        response
            .into_body()
            .take((JSON_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| {
                if writing {
                    uncertain()
                } else {
                    WriteError::Rejected("Could not read the YouTrack response.".into())
                }
            })?;
        if bytes.len() > JSON_LIMIT {
            return Err(if writing {
                uncertain()
            } else {
                WriteError::Rejected("YouTrack response exceeded the 8 MiB limit.".into())
            });
        }
        if !(200..300).contains(&status) {
            if writing && status >= 500 {
                return Err(uncertain());
            }
            let message = match status {
                401 => "YouTrack rejected this permanent token. Reconnect in Preferences → Integrations.".into(),
                403 => "YouTrack denied this action. Check the token scopes and project permissions.".into(),
                404 => "YouTrack project or issue was not found, or this account cannot access it.".into(),
                409 => "YouTrack rejected this change because the resource changed. Refresh and retry.".into(),
                429 => "YouTrack rate limit reached. Wait before retrying.".into(),
                301 | 302 | 303 | 307 | 308 => "YouTrack redirected the request. Check the service URL in Preferences.".into(),
                _ => error_text(&bytes, status, &self.token),
            };
            return Err(WriteError::Rejected(message));
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            if writing {
                uncertain()
            } else {
                WriteError::Rejected("YouTrack returned an invalid response.".into())
            }
        })
    }
}

pub(super) async fn download_attachment(
    y: &Youtrack,
    task: &TaskRef,
    id: &str,
) -> Result<Vec<u8>, String> {
    y.target(&task.project)?;
    validate_identifier(id, "attachment ID")?;
    let fresh = y.task(task).await?;
    let details = fresh
        .youtrack
        .as_ref()
        .ok_or("YouTrack returned no issue details.")?;
    let attachment = details
        .attachments
        .iter()
        .find(|a| a.id == id)
        .ok_or("Attachment no longer belongs to this task.")?;
    let service = Url::parse(&y.service).map_err(|_| "Invalid YouTrack service URL.")?;
    let origin = format!(
        "{}://{}{}",
        service.scheme(),
        service.host_str().ok_or("Invalid YouTrack service host.")?,
        service.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    let prefix = service.path().trim_end_matches('/');
    let full_uri = if let Some(url) = &attachment.url {
        let parsed = Url::parse(url)
            .or_else(|_| {
                let path = if url.starts_with('/') {
                    url.clone()
                } else {
                    format!("/{url}")
                };
                Url::parse(&format!("{origin}{path}"))
            })
            .map_err(|_| "YouTrack returned an invalid attachment URL.")?;
        if parsed.scheme() != service.scheme()
            || parsed.host_str() != service.host_str()
            || parsed.port() != service.port()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(
                "This attachment is hosted outside the configured YouTrack service.".into(),
            );
        }
        let path = parsed.path();
        let path = if path.starts_with(&format!("{prefix}/api/")) {
            path.to_owned()
        } else if path.starts_with("/api/") {
            format!("{prefix}{path}")
        } else {
            return Err("This attachment URL is outside the YouTrack API path.".into());
        };
        format!(
            "{origin}{path}{}",
            parsed.query().map(|q| format!("?{q}")).unwrap_or_default()
        )
    } else {
        format!(
            "{origin}{prefix}/api/issues/{}/attachments/{id}/file",
            encode_component(&details.internal_id)
        )
    };
    let mut authorization =
        gpui_kit::http_client::http::HeaderValue::from_str(&format!("Bearer {}", y.token))
            .map_err(|_| "Could not prepare credentials.")?;
    authorization.set_sensitive(true);
    let request = Request::builder()
        .method(method::GET)
        .uri(full_uri)
        .header("Authorization", authorization)
        .header("User-Agent", "Canopy-Desktop")
        .follow_redirects(RedirectPolicy::NoFollow)
        .timeout(Duration::from_secs(60))
        .body(AsyncBody::empty())
        .map_err(|_| "Could not prepare attachment download.")?;
    let response = y
        .http
        .send(request)
        .await
        .map_err(|_| "Could not download the YouTrack attachment.")?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!(
            "YouTrack attachment download failed (HTTP {status})."
        ));
    }
    let mut bytes = Vec::new();
    response
        .into_body()
        .take((ATTACHMENT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "Could not read the YouTrack attachment.")?;
    if bytes.len() > ATTACHMENT_LIMIT {
        return Err("Attachment exceeds the 25 MiB download limit.".into());
    }
    Ok(bytes)
}

fn encode_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                format!("{}", b as char).into_bytes()
            } else {
                format!("%{b:02X}").into_bytes()
            }
        })
        .map(char::from)
        .collect()
}

mod method {
    pub const GET: &str = "GET";
}

pub(super) fn component(value: &str) -> String {
    encode_component(value)
}
