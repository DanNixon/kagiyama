use anyhow::Result;
use kagiyama::{AlwaysReady, Watcher};
use prometheus_client::metrics::counter::Counter;
use tokio::time::{self, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    let mut watcher = Watcher::<AlwaysReady>::default();

    watcher.start_server("127.0.0.1:9090".parse()?).await;

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
                    Box::new(ticks.clone()),
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
                    Box::new(ticks.clone()),
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
