use super::readiness_probe::ReadinessProbe;
use hyper::{
    header::CONTENT_TYPE, service::service_fn, Body, Request, Response, Server, StatusCode,
};
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use serde::Serialize;
use std::{
    cmp::Eq,
    hash::Hash,
    marker::{Send, Sync},
    net::SocketAddr,
    ops::Deref,
    sync::{Arc, RwLock},
};
use strum::IntoEnumIterator;
use tokio::{sync::broadcast, task::JoinHandle};
use tower::make::Shared;

#[derive(Clone)]
pub struct Watcher<C: Hash + Eq + Send + Sync + Serialize> {
    metrics_registry: Arc<RwLock<Registry>>,
    readiness_probe: ReadinessProbe<C>,
    termination_signal: broadcast::Sender<()>,
}

impl<C: 'static + Clone + IntoEnumIterator + Hash + Eq + Sync + Send + Serialize> Default
    for Watcher<C>
{
    fn default() -> Self {
        let metrics_registry = Arc::new(RwLock::new(<Registry>::default()));
        let readiness_probe = ReadinessProbe::default();
        let (termination_signal, _) = broadcast::channel::<()>(1);

        metrics_registry.write().unwrap().register(
            "up",
            "Overall system readiness",
            readiness_probe.up.clone(),
        );

        Self {
            metrics_registry,
            readiness_probe,
            termination_signal,
        }
    }
}

impl<C: 'static + Clone + IntoEnumIterator + Hash + Eq + Sync + Send + Serialize> Watcher<C> {
    pub fn metrics_registry(&self) -> std::sync::RwLockWriteGuard<'_, Registry> {
        self.metrics_registry.write().unwrap()
    }

    pub fn readiness_probe(&self) -> ReadinessProbe<C> {
        self.readiness_probe.clone()
    }

    pub async fn start_server(&mut self, address: SocketAddr) -> JoinHandle<()> {
        let registry = self.metrics_registry.clone();
        let readiness_conditions = self.readiness_probe.clone();
        let mut termination_signal = self.termination_signal.subscribe();

        tokio::spawn(async move {
            let server =
                Server::bind(&address).serve(Shared::new(service_fn(move |req: Request<Body>| {
                    let registry = registry.clone();
                    let readiness_conditions = readiness_conditions.clone();

                    async move {
                        Ok::<_, anyhow::Error>(match req.uri().path() {
                            "/metrics" => {
                                let mut buffer = String::new();
                                encode(&mut buffer, &registry.read().unwrap())?;
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header(CONTENT_TYPE, "text/plain")
                                    .body(Body::from(buffer))
                                    .unwrap()
                            }
                            "/ready" => {
                                let ready = readiness_conditions.is_ready();
                                Response::builder()
                                    .status(match ready {
                                        true => StatusCode::OK,
                                        false => StatusCode::SERVICE_UNAVAILABLE,
                                    })
                                    .header(CONTENT_TYPE, "application/json")
                                    .body(Body::from(
                                        serde_json::to_string(
                                            readiness_conditions.conditions.read().unwrap().deref(),
                                        )
                                        .unwrap(),
                                    ))
                                    .unwrap()
                            }
                            "/alive" => Response::builder()
                                .status(StatusCode::OK)
                                .header(CONTENT_TYPE, "text/plain")
                                .body("alive".into())
                                .unwrap(),
                            _ => Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .header(CONTENT_TYPE, "text/plain")
                                .body("Not found".into())
                                .unwrap(),
                        })
                    }
                })));

            log::trace!("Listening on {}", address);
            let graceful = server.with_graceful_shutdown(async {
                termination_signal.recv().await.ok();
            });

            if let Err(e) = graceful.await {
                log::error!("Error running server: {}", e);
            }
        })
    }

    pub fn stop_server(&mut self) -> Result<usize, broadcast::error::SendError<()>> {
        log::trace!("Requesting server shutdown");
        self.termination_signal.send(())
    }
}
