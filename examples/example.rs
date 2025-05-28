use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use rtcache::Store;

struct TestStore {
    data: Arc<Mutex<HashMap<i32, String>>>,
}

impl TestStore {
    fn new(data: Arc<Mutex<HashMap<i32, String>>>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl Store<i32, Mutex<String>> for TestStore {
    async fn fetch(&self, key: &i32) -> anyhow::Result<Mutex<String>> {
        if let Some(value) = self.data.lock().await.get(key) {
            Ok(Mutex::new(value.clone()))
        } else {
            Err(anyhow::anyhow!("Key not found"))
        }
    }

    async fn update(&self, key: i32, value: Mutex<String>) {
        self.data.lock().await.insert(key, value.into_inner());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut data = HashMap::new();
    data.insert(1, String::from("Hello"));
    let data = Arc::new(Mutex::new(data));

    let mut cache =
        rtcache::Cache::new(TestStore::new(data.clone()), Duration::from_secs(60)).await;

    let v = cache.get(1).await.unwrap();

    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        *v.lock().await = String::from("World");
        drop(v);
    });

    cache.evict_all_sync().await;

    println!("{}", data.lock().await.get(&1).unwrap());

    Ok(())
}
