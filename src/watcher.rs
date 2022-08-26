use super::readiness_probe::ReadinessProbe;
use anyhow::{anyhow, Result};
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
    sync::Arc,
};
use strum::IntoEnumIterator;
use tokio::{sync::broadcast, task::JoinHandle};
use tower::make::Shared;

#[derive(Clone)]
pub struct Watcher<C: Hash + Eq + Send + Sync + Serialize> {
    metric_registry: Arc<Registry>,
    readiness_probe: ReadinessProbe<C>,
    termination_signal: broadcast::Sender<()>,
}

impl<C: 'static + Clone + IntoEnumIterator + Hash + Eq + Sync + Send + Serialize> Default
    for Watcher<C>
{
    fn default() -> Self {
        let mut metric_registry = <Registry>::default();
        let readiness_probe = ReadinessProbe::default();
        let (termination_signal, _) = broadcast::channel::<()>(1);

        metric_registry.register("up", "Overall system readiness", readiness_probe.up.clone());

        let metric_registry = Arc::new(metric_registry);

        Self {
            metric_registry,
            readiness_probe,
            termination_signal,
        }
    }
}

impl<C: 'static + Clone + IntoEnumIterator + Hash + Eq + Sync + Send + Serialize> Watcher<C> {
    pub fn sub_registry<P: std::convert::AsRef<str>>(
        &mut self,
        prefix: P,
    ) -> Result<&mut Registry> {
        match Arc::get_mut(&mut self.metric_registry) {
            Some(registry) => Ok(registry.sub_registry_with_prefix(prefix)),
            None => Err(anyhow!("Cannot lock registry")),
        }
    }

    pub fn readiness_conditions(&self) -> ReadinessProbe<C> {
        self.readiness_probe.clone()
    }

    pub async fn start_server(&mut self, address: SocketAddr) -> Result<JoinHandle<()>> {
        let registry = self.metric_registry.clone();
        let readiness_conditions = self.readiness_probe.clone();
        let mut termination_signal = self.termination_signal.subscribe();

        Ok(tokio::spawn(async move {
            let server =
                Server::bind(&address).serve(Shared::new(service_fn(move |req: Request<Body>| {
                    let registry = registry.clone();
                    let readiness_conditions = readiness_conditions.clone();

                    async move {
                        Ok::<_, anyhow::Error>(match req.uri().path() {
                            "/metrics" => {
                                let mut buffer = vec![];
                                encode(&mut buffer, &registry)?;
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
        }))
    }

    pub fn stop_server(&mut self) -> Result<()> {
        log::trace!("Requesting server shutdown");

        match self.termination_signal.send(()) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!(e)),
        }
    }
}
