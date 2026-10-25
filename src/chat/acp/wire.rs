//! ACP v1 JSON-RPC framing. IDs are opaque strings or numbers, never UUIDs.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(serde_json::Number),
}

#[derive(Clone, Debug)]
pub enum Message {
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: RequestId,
        result: Result<Value, Value>,
    },
}
pub fn parse_line(line: &[u8]) -> Result<Option<Message>, String> {
    if line.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let v: Value = serde_json::from_slice(line).map_err(|_| "Invalid ACP JSON.".to_owned())?;
    if v["jsonrpc"] != "2.0" {
        return Err("Invalid ACP JSON-RPC version.".into());
    }
    let id = v
        .get("id")
        .map(|id| serde_json::from_value::<RequestId>(id.clone()))
        .transpose()
        .map_err(|_| "Invalid ACP request ID.".to_owned())?;
    if let Some(method) = v["method"].as_str() {
        let params = v.get("params").cloned().unwrap_or_else(|| json!({}));
        return Ok(Some(match id {
            Some(id) => Message::Request {
                id,
                method: method.into(),
                params,
            },
            None => Message::Notification {
                method: method.into(),
                params,
            },
        }));
    }
    let id = id.ok_or("ACP response has no ID.")?;
    let result = match (v.get("result"), v.get("error")) {
        (Some(result), None) => Ok(result.clone()),
        (None, Some(error)) => Err(error.clone()),
        _ => return Err("ACP response must contain a result or an error.".into()),
    };
    Ok(Some(Message::Response { id, result }))
}
pub fn request(id: &RequestId, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
pub fn notify(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}
pub fn reply(id: &RequestId, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
pub fn error(id: &RequestId, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
