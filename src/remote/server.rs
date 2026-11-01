//! One listener and executor owned by the running application. Closing an RPC
//! does not stop its effects; shutdown drains owned work before dropping Tokio.
use super::{
    auth::RpcAuth,
    enrollment::EnrollmentRpc,
    host::{HealthRpc, HostRpc, HostState},
    resources::ResourceLocks,
    sync::ReplayHub,
    tls::HostIdentity,
};
use crate::api::proto;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{service::Routes, transport::Server};
#[cfg(test)]
mod tests;

pub struct RunningServer {
    address: SocketAddr,
    stop: async_channel::Sender<()>,
    finished: async_channel::Receiver<Result<(), String>>,
    state: HostState,
}
impl RunningServer {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn state(&self) -> &HostState {
        &self.state
    }
    /// The app waits for this before closing the metadata writer or quitting.
    pub async fn stop(self) -> Result<(), String> {
        self.state
            .set(proto::HostReadiness::Draining, String::new());
        let _ = self.stop.try_send(());
        self.finished
            .recv()
            .await
            .map_err(|_| "Remote host stopped without a shutdown result.".to_owned())?
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
    }
}

pub struct ServerServices {
    pub auth: RpcAuth,
    pub state: HostState,
    pub identity: Arc<HostIdentity>,
    pub enrollment: EnrollmentRpc,
    pub replay: ReplayHub,
    pub operations: ResourceLocks,
    /// Concrete typed feature services. This is a tonic router, not a domain
    /// command dispatcher; no method name or JSON payload is interpreted here.
    pub features: Routes,
    /// The same concrete service owns the staged-secret cleanup lifecycle.
    pub settings: super::settings_rpc::SettingsRpc,
}

pub async fn start(address: SocketAddr, services: ServerServices) -> Result<RunningServer, String> {
    let (started, start_result) = async_channel::bounded(1);
    let (stop, stopping) = async_channel::bounded(1);
    let (finished, completion) = async_channel::bounded(1);
    let state = services.state.clone();
    std::thread::Builder::new()
        .name("canopy-remote".into())
        .spawn(move || {
            let startup = started.clone();
            let result = (|| {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .map_err(|_| "Could not start the remote executor.".to_owned())?;
                runtime.block_on(async move {
                    services.settings.cleanup().await.map_err(|_| "Remote secret cleanup failed; listener was not opened.".to_owned())?;
                    let listener = tokio::net::TcpListener::bind(address)
                        .await
                        .map_err(|_| "Could not bind the selected remote address.".to_owned())?;
                    let bound = listener
                        .local_addr()
                        .map_err(|_| "Remote listener address is unavailable.".to_owned())?;
                    let host = HostRpc::new(
                        services.auth.clone(),
                        services.state.clone(),
                        services.replay,
                        services.operations.clone(),
                    );
                    let health = HealthRpc::new(services.auth, services.state.clone());
                    let routes = services
                        .features
                        .add_service(
                            proto::host_service_server::HostServiceServer::new(host)
                                .max_decoding_message_size(crate::host::CONTROL_BYTES)
                                .max_encoding_message_size(crate::host::CONTROL_BYTES),
                        )
                        .add_service(
                            crate::api::proto::enrollment_service_server::EnrollmentServiceServer::new(
                                services.enrollment,
                            )
                            .max_decoding_message_size(64 * 1024)
                            .max_encoding_message_size(64 * 1024),
                        )
                        .add_service(
                            tonic_health::pb::health_server::HealthServer::new(health)
                                .max_decoding_message_size(1024)
                                .max_encoding_message_size(1024),
                        );
                    let server = Server::builder()
                        .tls_config(services.identity.server_config())
                        .map_err(|_| "Could not configure remote TLS.".to_owned())?;
                    let router = server
                        .concurrency_limit_per_connection(64)
                        .max_concurrent_streams(128)
                        .initial_stream_window_size(256 * 1024)
                        .initial_connection_window_size(1024 * 1024)
                        .http2_keepalive_interval(Some(Duration::from_secs(30)))
                        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
                        .add_routes(routes);
                    let maintenance = services.settings.clone();
                    let maintenance_state = services.state.clone();
                    let maintenance_operations = services.operations.clone();
                    let maintenance = tokio::spawn(async move {
                        let mut timer = tokio::time::interval(Duration::from_secs(60));
                        timer.tick().await;
                        loop { timer.tick().await;
                            if maintenance_state.accepting().is_err() { return; }
                            if maintenance.cleanup().await.is_err() {
                            if maintenance_state.accepting().is_err() { return; }
                            maintenance_state.set(proto::HostReadiness::StorageFault, "Remote settings cleanup failed.".into());
                            let _ = maintenance_operations.drain().await;
                            return;
                        } }
                    });
                    services
                        .state
                        .set(proto::HostReadiness::Ready, String::new());
                    if startup.send(Ok(bound)).await.is_err() {
                        return Ok(());
                    }
                    let state = services.state.clone();
                    let fault_state = state.clone();
                    let fault_operations = services.operations.clone();
                    let abort_maintenance = maintenance.abort_handle();
                    let (drained, drain_result) = tokio::sync::oneshot::channel();
                    let shutdown = async move {
                        let _ = stopping.recv().await;
                        maintenance.abort();
                        state.set(proto::HostReadiness::Draining, String::new());
                        // Refuse admission immediately, let already accepted effects
                        // commit their final outcomes, then stop network/async I/O.
                        let result = services.operations.drain().await;
                        if result.is_err() {
                            state.set(
                                proto::HostReadiness::StorageFault,
                                "Remote operation drain failed.".into(),
                            );
                        }
                        let _ = drained.send(result);
                    };
                    let serving = router.serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown);
                    tokio::pin!(serving);
                    let result = tokio::select! {
                        result = &mut serving => result.map_err(|_| "The remote listener stopped unexpectedly.".to_owned()),
                        result = drain_result => {
                            result.map_err(|_| "Remote shutdown stopped before operation drain.".to_owned())?
                                .map_err(|_| "Could not drain remote operations.".to_owned())?;
                            // All accepted effects are settled. Idle or non-reading
                            // clients cannot keep the app alive by holding a body open.
                            match tokio::time::timeout(Duration::from_secs(5), &mut serving).await {
                                Ok(result) => result.map_err(|_| "The remote listener stopped unexpectedly.".to_owned()),
                                Err(_) => Ok(()),
                            }
                        }
                    };
                    if let Err(error) = &result {
                        abort_maintenance.abort();
                        fault_state.set(proto::HostReadiness::StorageFault,error.clone());
                        // Keep the executor alive until already accepted effects
                        // settle, even when the listener itself fails.
                        let _ = fault_operations.drain().await;
                    }
                    result
                })
            })();
            // Also reports errors that happened before startup; no caller waits on a
            // channel whose sender was silently dropped.
            if let Err(error) = &result {
                let _ = started.try_send(Err(error.clone()));
            }
            let _ = finished.try_send(result);
        })
        .map_err(|_| "Could not create the remote host thread.".to_owned())?;
    let address = start_result
        .recv()
        .await
        .map_err(|_| "Remote startup stopped.".to_owned())??;
    Ok(RunningServer {
        address,
        stop,
        finished: completion,
        state,
    })
}
