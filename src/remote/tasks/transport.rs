//! Recheck the host grant at each provider request, including writes after an
//! adapter's asynchronous workflow/schema preflight. A revoked device cannot
//! retain dispatch authority merely by holding an already constructed client.
use super::*;
use gpui_kit::http_client::{
    AsyncBody, Request as HttpRequest, Response as HttpResponse, Url, http::HeaderValue,
};
use std::pin::Pin;

struct AuthorizedHttp {
    inner: Arc<dyn HttpClient>,
    auth: RpcAuth,
    owner: String,
    root: String,
    account: i::Account,
    scope: u32,
}
impl HttpClient for AuthorizedHttp {
    fn user_agent(&self) -> Option<&HeaderValue> {
        self.inner.user_agent()
    }
    fn proxy(&self) -> Option<&Url> {
        self.inner.proxy()
    }
    fn send(
        &self,
        request: HttpRequest<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = gpui_kit::http_client::Result<HttpResponse<AsyncBody>>> + Send>>
    {
        let inner = self.inner.clone();
        let auth = self.auth.clone();
        let owner = self.owner.clone();
        let root = self.root.clone();
        let account = self.account.clone();
        let scope = self.scope;
        Box::pin(async move {
            auth.resource(&owner, scope, &root).await.map_err(|_| {
                gpui_kit::http_client::anyhow!("Task dispatch authorization was revoked.")
            })?;
            let current =
                auth.client().load_integrations().await.map_err(|_| {
                    gpui_kit::http_client::anyhow!("Integration registry unavailable.")
                })?;
            if !current.accounts.iter().any(|a| a == &account) {
                return Err(gpui_kit::http_client::anyhow!(
                    "Integration changed before dispatch."
                ));
            }
            inner.send(request).await
        })
    }
}
impl TasksRpc {
    pub(super) fn scoped_http(
        &self,
        account: &i::Account,
        g: &DeviceGrant,
        root: &str,
    ) -> Arc<dyn HttpClient> {
        Arc::new(AuthorizedHttp {
            inner: self.http.clone(),
            auth: self.auth.clone(),
            owner: g.id().into(),
            root: root.into(),
            account: account.clone(),
            scope: TASKS,
        })
    }
    pub(super) fn read_http(
        &self,
        account: &i::Account,
        g: &DeviceGrant,
        root: &str,
    ) -> Arc<dyn HttpClient> {
        Arc::new(AuthorizedHttp {
            inner: self.http.clone(),
            auth: self.auth.clone(),
            owner: g.id().into(),
            root: root.into(),
            account: account.clone(),
            scope: Self::read_scope(g),
        })
    }
}
