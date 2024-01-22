use std::error::Error;

use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cache = thru::Cache::new(
        |key: i32| async move {
            println!("[Fetch] key: {}", key);
            Ok(Mutex::new(String::from("Hello")))
        },
        |key, value| async move {
            println!("[Update] key: {}, value: {}", key, value.lock().await);
        },
    )
    .await;

    let v = cache.get(12).await.unwrap();

    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        *v.lock().await = String::from("World");
        drop(v);
    });

    cache.evict_all_sync().await;

    Ok(())
}
