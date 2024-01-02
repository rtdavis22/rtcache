use std::error::Error;

use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cache = thru::Cache::new().await;

    let v = cache.get(12).await;
    drop(v);

    let v = cache.get(12).await;

    tokio::spawn(async move {
        sleep(Duration::from_secs(3)).await;
        drop(v);
    });

    cache.evict_all_sync().await;

    Ok(())
}
