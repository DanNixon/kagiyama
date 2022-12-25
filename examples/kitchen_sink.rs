use anyhow::Result;
use kagiyama::Watcher;
use prometheus_client::metrics::counter::Counter;
use serde::Serialize;
use strum_macros::EnumIter;
use tokio::time::{self, Duration};

#[derive(Clone, Serialize, EnumIter, PartialEq, Hash, Eq)]
enum ReadinessConditions {
    One,
    Two,
    Three,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut watcher = Watcher::<ReadinessConditions>::default();

    watcher.start_server("127.0.0.1:9090".parse()?).await;

    // Simulate a subsystem starting up
    let mut readiness_conditions = watcher.readiness_probe();
    tokio::spawn(async move {
        time::sleep(Duration::from_secs(2)).await;
        readiness_conditions.mark_ready(ReadinessConditions::One);
    });

    // Simulate a subsystem starting up
    let mut readiness_conditions = watcher.readiness_probe();
    tokio::spawn(async move {
        time::sleep(Duration::from_secs(6)).await;
        readiness_conditions.mark_ready(ReadinessConditions::Two);
    });

    // Simulate an unstable condition
    let mut readiness_conditions = watcher.readiness_probe();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            readiness_conditions.mark_ready(ReadinessConditions::Three);
            interval.tick().await;
            readiness_conditions.mark_not_ready(ReadinessConditions::Three);
        }
    });

    // Simulate a metric
    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(1));

            let ticks = Counter::<u64>::default();

            {
                let mut registry = watcher.metrics_registry();
                registry.register(
                    "ticks",
                    "A demo metric, counts up every second",
                    ticks.clone(),
                );
            }

            loop {
                interval.tick().await;
                ticks.inc();
            }
        });
    }

    // Simulate a metric
    {
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(2));

            let ticks = Counter::<u64>::default();

            {
                let mut registry = watcher.metrics_registry();
                let registry = registry.sub_registry_with_prefix("extra_things");
                registry.register(
                    "ticks",
                    "A demo metric, counts up every two seconds",
                    ticks.clone(),
                );
            }

            loop {
                interval.tick().await;
                ticks.inc();
            }
        });
    }

    time::sleep(Duration::from_secs(300)).await;

    Ok(())
}
