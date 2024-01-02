use std::error::Error;

use async_trait::async_trait;
use tokio::time::{sleep, Duration};

use thru::Store;

struct TestStore;

#[async_trait]
impl Store<i32, String> for TestStore {
    async fn fetch(&self, key: &i32) -> String {
        println!("[Fetch] key: {}", key);
        String::from("Hello")
    }

    async fn update(&self, key: i32, value: String) {
        println!("[Update] key: {}, value: {}", key, value);
        // if list is dirty, update store
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cache = thru::Cache::new(TestStore).await;

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
