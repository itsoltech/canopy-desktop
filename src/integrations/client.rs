use super::*;
use gpui_kit::http_client::HttpClient;
use std::sync::Arc;
/// Credentials are selected by the trusted connection registry, never by a response URL.
pub fn client(
    http: Arc<dyn HttpClient>,
    account: &Account,
    token: String,
) -> Result<Box<dyn TaskProvider>, String> {
    match (&account.provider, &account.scope) {
        (Provider::Github, AccountScope::Default | AccountScope::Owner(_)) => {
            Ok(Box::new(github::Github::new(http, token)))
        }
        (
            Provider::Jira,
            AccountScope::Jira {
                site,
                email,
                cloud_id,
            },
        ) => Ok(Box::new(jira::Jira::new(
            http,
            site,
            email,
            cloud_id.as_deref(),
            token,
            account.login.clone(),
        )?)),
        (Provider::Youtrack, AccountScope::Youtrack { service }) => Ok(Box::new(
            youtrack::Youtrack::new(http, service, token, account.login.clone())?,
        )),
        _ => Err("The connection does not match this task provider.".into()),
    }
}
