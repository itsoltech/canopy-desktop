use super::*;
use base64::Engine;
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpRequestExt, RedirectPolicy, Request};
use std::time::Duration;
impl Jira {
    pub(super) async fn get(&self, path: &str) -> Result<Value, String> {
        self.send("GET", path, None, false)
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
        if !path.starts_with("/rest/") || path.contains("..") || path.contains('#') {
            return Err("Invalid Jira API path.".into());
        }
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.email, self.token));
        let mut auth =
            gpui_kit::http_client::http::HeaderValue::from_str(&format!("Basic {encoded}"))
                .map_err(|_| "Could not prepare Jira credentials.")?;
        auth.set_sensitive(true);
        let req = Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.api))
            .header("Authorization", auth)
            .header("Accept", "application/json")
            .header("Content-Type", mime)
            .header("User-Agent", "Canopy-Desktop")
            .header("X-Atlassian-Token", "no-check")
            .follow_redirects(RedirectPolicy::NoFollow)
            .timeout(Duration::from_secs(30))
            .body(body.map(AsyncBody::from).unwrap_or_else(AsyncBody::empty))
            .map_err(|_| "Could not prepare Jira request.")?;
        let uncertain = || {
            WriteError::Uncertain("Could not confirm the Jira change. It may already be saved; refresh the task before retrying.".into())
        };
        let response = self.http.send(req).await.map_err(|_| {
            if writing {
                uncertain()
            } else {
                WriteError::Rejected(
                    "Could not reach Jira. Check the site address and connection.".into(),
                )
            }
        })?;
        let code = response.status().as_u16();
        if code == 204 {
            return Ok(Value::Null);
        }
        if writing && code >= 500 {
            return Err(uncertain());
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| {
                if writing && code < 300 {
                    uncertain()
                } else {
                    WriteError::Rejected("Could not read Jira response.".into())
                }
            })?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(if writing && code < 300 {
                uncertain()
            } else {
                WriteError::Rejected(
                    "Jira response exceeded the 8 MiB limit. Narrow your search.".into(),
                )
            });
        }
        if !(200..300).contains(&code) {
            let msg=match code {
                401=>"Jira rejected these credentials. Check the email, token and token type (with or without scopes).".into(),
                403=>"Jira denied this action. Check account permissions and API token scopes.".into(),
                404=>"Jira project or task was not found, or this account cannot access it.".into(),
                429=>"Jira rate limit reached. Wait before retrying.".into(),
                301|302|303|307|308=>"Jira redirected the request. Check the site address in Preferences.".into(),
                400|409|422=> {
                    let v:Value=serde_json::from_slice(&bytes).unwrap_or_default();
                    let mut parts:Vec<String>=v["errorMessages"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
                    if let Some(fields)=v["errors"].as_object(){parts.extend(fields.iter().filter_map(|(k,v)|v.as_str().map(|s|format!("{k}: {s}"))));}
                    let detail=parts.join(" ").replace(&self.token,"[redacted]").replace(&encoded,"[redacted]");
                    format!("Jira could not save these fields. {}",detail.chars().filter(|c|!c.is_control()||*c=='\n').take(1800).collect::<String>())
                },
                _=>format!("Jira request failed (HTTP {code})."),
            };
            return Err(WriteError::Rejected(msg));
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            if writing {
                uncertain()
            } else {
                WriteError::Rejected("Jira returned an invalid response.".into())
            }
        })
    }
}
impl Jira {
    pub async fn download_attachment(&self, task: &TaskRef, id: &str) -> Result<Vec<u8>, String> {
        self.target(&task.project)?;
        ident(id)?;
        let fresh = self.task(task).await?;
        if !fresh.jira.as_ref().is_some_and(|d| {
            d.fields["attachment"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|a| a["id"].as_str() == Some(id))
        }) {
            return Err("Attachment no longer belongs to this task.".into());
        }
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.email, self.token));
        let mut auth =
            gpui_kit::http_client::http::HeaderValue::from_str(&format!("Basic {encoded}"))
                .map_err(|_| "Could not prepare credentials.")?;
        auth.set_sensitive(true);
        let req = Request::builder()
            .uri(format!(
                "{}/rest/api/3/attachment/content/{id}?redirect=false",
                self.api
            ))
            .header("Authorization", auth)
            .follow_redirects(RedirectPolicy::NoFollow)
            .timeout(Duration::from_secs(60))
            .body(AsyncBody::empty())
            .map_err(|_| "Could not prepare download.")?;
        let response = self
            .http
            .send(req)
            .await
            .map_err(|_| "Could not download the attachment.")?;
        if response.status().as_u16() != 200 {
            return Err(format!(
                "Jira attachment download failed (HTTP {}).",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .take(25 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Could not read the attachment.")?;
        if bytes.len() > 25 * 1024 * 1024 {
            return Err("Attachment exceeds the 25 MiB download limit.".into());
        }
        Ok(bytes)
    }
}
