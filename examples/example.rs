use std::error::Error;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use thru::Store;

struct TestStore;

#[async_trait]
impl Store<i32, Mutex<String>> for TestStore {
    async fn fetch(&self, key: &i32) -> Mutex<String> {
        println!("[Fetch] key: {}", key);
        Mutex::new(String::from("Hello"))
    }

    async fn update(&self, key: i32, value: Mutex<String>) {
        println!("[Update] key: {}, value: {}", key, value.lock().await);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut cache = thru::Cache::new(TestStore).await;

    let v = cache.get(12).await.unwrap();

    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        *v.lock().await = String::from("World");
        drop(v);
    });

    cache.evict_all_sync().await;

    Ok(())
}
