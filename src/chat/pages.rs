//! The page service (DEC-077): `preview_html` from an agent's MCP server asks Canopy to
//! render a page off screen and gets back its image, content height and console. A
//! private Unix socket with a token per chat session; each request waits for its
//! answer. Rendering itself happens in the app (WKWebView on the main thread).
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

/// Environment of the MCP server: where and how to reach the service.
pub const SOCKET_ENV: &str = "CANOPY_PAGES_SOCKET";
pub const TOKEN_ENV: &str = "CANOPY_PAGES_TOKEN";
/// How long a preview may take before the agent hears it failed.
pub const DEADLINE: Duration = Duration::from_secs(30);
const MAX_REQUEST: usize = 2 * 1024 * 1024;
const MAX_RESPONSE: usize = 48 * 1024 * 1024;

/// A rendered page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    #[serde(with = "png")]
    pub png: Vec<u8>,
    pub height: f32,
    pub console: Vec<String>,
}

mod png {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
struct Wire {
    token: String,
    html: String,
    width: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Answer {
    Ok(Preview),
    Error(String),
}

/// A page to render for an agent; answer it with `reply`.
pub struct Request {
    pub html: String,
    pub width: f32,
    reply: std::sync::mpsc::SyncSender<Result<Preview, String>>,
}

impl Request {
    pub fn reply(self, result: Result<Preview, String>) {
        let _ = self.reply.send(result);
    }
}

/// What a chat session's MCP server needs to reach the service.
#[derive(Clone, Debug)]
pub struct Access {
    pub socket: PathBuf,
    pub token: String,
}

pub struct PageService {
    registry: Registry,
    _dir: tempfile::TempDir,
}

/// Hands out and takes back session tokens; cheap to clone and usable from anywhere,
/// so a chat starting or closing needs nothing else of the app.
#[derive(Clone)]
pub struct Registry {
    socket: PathBuf,
    tokens: Arc<Mutex<HashSet<String>>>,
}

impl Registry {
    /// A token for one chat session.
    pub fn register(&self) -> Access {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(token.clone());
        }
        Access {
            socket: self.socket.clone(),
            token,
        }
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.remove(token);
        }
    }
}

impl PageService {
    /// Listens on a private socket; requests arrive on the receiver.
    #[cfg(unix)]
    pub fn start() -> Result<(Self, async_channel::Receiver<Request>), String> {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        // /tmp keeps the path within macOS's short socket address limit.
        let dir = tempfile::Builder::new()
            .prefix("canopy-pages-")
            .tempdir_in("/tmp")
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let socket = dir.path().join("pages.sock");
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        let tokens = Arc::new(Mutex::new(HashSet::new()));
        let (requests, receiver) = async_channel::bounded(16);
        let known = tokens.clone();
        std::thread::Builder::new()
            .name("canopy-pages".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else {
                        break;
                    };
                    let known = known.clone();
                    let requests = requests.clone();
                    std::thread::spawn(move || serve(stream, &known, &requests));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok((
            Self {
                registry: Registry { socket, tokens },
                _dir: dir,
            },
            receiver,
        ))
    }

    #[cfg(not(unix))]
    pub fn start() -> Result<(Self, async_channel::Receiver<Request>), String> {
        Err("Page previews need macOS.".into())
    }

    pub fn registry(&self) -> Registry {
        self.registry.clone()
    }
}

#[cfg(unix)]
fn serve(
    mut stream: std::os::unix::net::UnixStream,
    tokens: &Mutex<HashSet<String>>,
    requests: &async_channel::Sender<Request>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let answer = (|| {
        let wire: Wire = serde_json::from_slice(&read_frame(&mut stream, MAX_REQUEST)?)
            .map_err(|_| "Invalid page request.".to_owned())?;
        let known = tokens
            .lock()
            .map(|tokens| tokens.iter().any(|token| same(token, &wire.token)))
            .unwrap_or(false);
        if !known {
            return Err("This chat can't preview pages any more.".to_owned());
        }
        let (reply, answer) = std::sync::mpsc::sync_channel(1);
        requests
            .send_blocking(Request {
                html: wire.html,
                width: wire.width,
                reply,
            })
            .map_err(|_| "Canopy is closing.".to_owned())?;
        answer
            .recv_timeout(DEADLINE)
            .map_err(|_| "The page took too long to render.".to_owned())?
    })();
    let bytes = serde_json::to_vec(&match answer {
        Ok(preview) => Answer::Ok(preview),
        Err(error) => Answer::Error(error),
    })
    .unwrap_or_default();
    let _ = write_frame(&mut stream, &bytes);
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

fn read_frame(stream: &mut impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .map_err(|_| "The page service closed the connection.".to_owned())?;
    let length = u32::from_be_bytes(length) as usize;
    if length > limit {
        return Err("The page is too large.".into());
    }
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .map_err(|_| "The page service closed the connection.".to_owned())?;
    Ok(bytes)
}

fn write_frame(stream: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

/// Asks the service at `socket` to render `html` `width` wide (the MCP server's side).
#[cfg(unix)]
pub fn preview(socket: &Path, token: &str, html: &str, width: f32) -> Result<Preview, String> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket)
        .map_err(|_| "Canopy isn't running for this chat.".to_owned())?;
    let _ = stream.set_read_timeout(Some(DEADLINE + Duration::from_secs(5)));
    let request = serde_json::to_vec(&Wire {
        token: token.to_owned(),
        html: html.to_owned(),
        width,
    })
    .map_err(|e| e.to_string())?;
    write_frame(&mut stream, &request).map_err(|e| e.to_string())?;
    match serde_json::from_slice(&read_frame(&mut stream, MAX_RESPONSE)?)
        .map_err(|_| "Invalid answer from Canopy.".to_owned())?
    {
        Answer::Ok(preview) => Ok(preview),
        Answer::Error(error) => Err(error),
    }
}

#[cfg(not(unix))]
pub fn preview(_: &Path, _: &str, _: &str, _: f32) -> Result<Preview, String> {
    Err("Page previews need macOS.".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn requests_need_a_token_and_get_their_answer() {
        let (service, requests) = PageService::start().unwrap();
        let registry = service.registry();
        let access = registry.register();
        // The app's side: answers each request once.
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv_blocking() {
                let height = request.width / 2.;
                let console = vec![format!("log: {}", request.html)];
                request.reply(Ok(Preview {
                    png: vec![1, 2, 3],
                    height,
                    console,
                }));
            }
        });
        let preview = preview(&access.socket, &access.token, "<p>x</p>", 400.).unwrap();
        assert_eq!(preview.png, [1, 2, 3]);
        assert_eq!(preview.height, 200.);
        assert_eq!(preview.console, ["log: <p>x</p>"]);
        let refused = super::preview(&access.socket, "wrong", "<p>x</p>", 400.).unwrap_err();
        assert!(refused.contains("can't preview"), "{refused}");
        registry.remove(&access.token);
        assert!(super::preview(&access.socket, &access.token, "x", 1.).is_err());
    }
}
