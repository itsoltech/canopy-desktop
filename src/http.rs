//! The desktop bootstrap must install a real transport: GPUI defaults to a null client.
use futures_lite::io::AsyncRead;
use gpui_kit::http_client::{
    AsyncBody, HttpClient, Request, Response, Result, Url, http::HeaderValue,
};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

pub fn new_client() -> std::result::Result<Arc<dyn HttpClient>, &'static str> {
    // Uses the platform certificate verifier and normal proxy discovery.
    reqwest_client::ReqwestClient::proxy_user_agent_and_read_timeout(
        None,
        "Canopy-Desktop",
        Some(Duration::from_secs(20)),
    )
    .map(|inner| Arc::new(DesktopHttpClient { inner }) as Arc<dyn HttpClient>)
    .map_err(|_| "Could not initialize Canopy's HTTP transport.")
}

struct DesktopHttpClient {
    inner: reqwest_client::ReqwestClient,
}
impl HttpClient for DesktopHttpClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        self.inner.user_agent()
    }
    fn proxy(&self) -> Option<&Url> {
        self.inner.proxy()
    }
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<AsyncBody>>> + Send>> {
        let response = self.inner.send(request);
        Box::pin(async move {
            let (parts, body) = response.await?.into_parts();
            Ok(Response::from_parts(
                parts,
                AsyncBody::from_reader(RuntimeBody(body)),
            ))
        })
    }
}

// gpui-pre-reqwest-client 0.3.8 runs send() on Tokio but returns the body stream
// to GPUI. Reqwest creates body-read timers when polled, which panics without a
// Tokio context. Enter its shared runtime only for each poll, never across await.
// Keep streaming so callers can enforce response-size bounds and cancel reads.
struct RuntimeBody(AsyncBody);
impl AsyncRead for RuntimeBody {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let _runtime = reqwest_client::runtime().enter();
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}
