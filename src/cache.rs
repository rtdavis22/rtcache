use std::collections::{hash_map, HashMap};
use std::error;
use std::fmt;
use std::mem;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{sleep, Duration, Instant};

#[async_trait]
pub trait Store<K, V> {
    async fn fetch(&self, key: &K) -> anyhow::Result<V>;
    async fn update(&self, key: K, value: V);
}

#[derive(Debug)]
struct RealCacheNode<V> {
    value: Arc<V>,
    #[allow(dead_code)]
    first_access_ts: Instant,
    last_access_ts: Instant,
}

impl<V> RealCacheNode<V> {
    fn new(value: Arc<V>) -> Self {
        let now = Instant::now();
        Self {
            value,
            first_access_ts: now,
            last_access_ts: now,
        }
    }

    fn try_unwrap(mut self) -> Result<V, Self> {
        match Arc::try_unwrap(self.value) {
            Ok(value) => Ok(value),
            Err(arc) => {
                self.value = arc;
                Err(self)
            }
        }
    }

    fn bump_access_time(&mut self) {
        self.last_access_ts = Instant::now();
    }
}

#[derive(Debug)]
enum CacheNode<V> {
    Real(RealCacheNode<V>),
    Dummy,
}

impl<V> CacheNode<V> {
    fn new(value: Arc<V>) -> Self {
        Self::Real(RealCacheNode::new(value))
    }

    fn unwrap(&self) -> &RealCacheNode<V> {
        match self {
            Self::Real(real) => real,
            Self::Dummy => unreachable!(),
        }
    }

    fn unwrap_mut(&mut self) -> &mut RealCacheNode<V> {
        match self {
            Self::Real(real) => real,
            Self::Dummy => unreachable!(),
        }
    }
}

#[derive(Debug)]
enum CacheEntry<V> {
    Fetching(broadcast::Sender<Result<Arc<V>, Arc<anyhow::Error>>>),
    FetchFailed(Arc<anyhow::Error>),
    Node(CacheNode<V>),
}

#[derive(Debug)]
pub struct GetError {
    pub fetch_error: Arc<anyhow::Error>,
}

impl GetError {
    fn new(fetch_error: Arc<anyhow::Error>) -> Self {
        Self { fetch_error }
    }
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to fetch")
    }
}

impl error::Error for GetError {}

pub struct Cache<K, V> {
    data: Arc<Mutex<HashMap<K, CacheEntry<V>>>>,
    pruner_join_handle: tokio::task::JoinHandle<()>,
    store: Arc<dyn Store<K, V> + Send + Sync>,
}

impl<K, V> Cache<K, V>
where
    K: std::hash::Hash + fmt::Display + Copy + Eq + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    pub async fn new(store: impl Store<K, V> + Send + Sync + 'static, ttl: Duration) -> Self {
        let store = Arc::new(store);
        let data = Arc::new(Mutex::new(HashMap::new()));
        let pruner_join_handle = Self::pruner_join_handle(data.clone(), store.clone(), ttl);

        Self {
            data,
            pruner_join_handle,
            store,
        }
    }

    pub async fn get(&self, k: K) -> Result<Arc<V>, GetError> {
        let data = self.data.clone();
        let mut lock = self.data.lock().await;

        match lock.get_mut(&k) {
            None => {
                let (tx, mut rx) = broadcast::channel(1);
                lock.insert(k, CacheEntry::Fetching(tx.clone()));
                drop(lock);

                let store_clone = self.store.clone();
                tokio::spawn(async move {
                    let fetch_result = store_clone.fetch(&k).await.map(Arc::new).map_err(Arc::new);

                    let mut data = data.lock().await;
                    let result = match data.entry(k) {
                        hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                            // This could mean that the key was inserted while the
                            // fetch was happening. In this case, we ignore the fetched
                            // value and return the inserted value.
                            CacheEntry::Node(ref mut node) => {
                                let real_node = node.unwrap_mut();
                                real_node.bump_access_time();
                                Ok(real_node.value.clone())
                            }
                            CacheEntry::Fetching(_) | CacheEntry::FetchFailed(_) => {
                                match fetch_result {
                                    Ok(value) => {
                                        e.insert(CacheEntry::Node(CacheNode::new(value.clone())));
                                        Ok(value)
                                    }
                                    Err(err) => {
                                        e.insert(CacheEntry::FetchFailed(err.clone()));
                                        Err(err)
                                    }
                                }
                            }
                        },
                        // This can happen if the value in the cache was deleted while
                        // the fetch was happening.
                        hash_map::Entry::Vacant(e) => match fetch_result {
                            Ok(value) => {
                                e.insert(CacheEntry::Node(CacheNode::new(value.clone())));
                                Ok(value)
                            }
                            Err(err) => {
                                e.insert(CacheEntry::FetchFailed(err.clone()));
                                Err(err)
                            }
                        },
                    };
                    drop(data);

                    let _ = tx.send(result);
                });

                rx.recv().await.unwrap().map_err(GetError::new)
            }
            Some(CacheEntry::Fetching(tx)) => {
                let mut rx = tx.subscribe();
                drop(lock);
                rx.recv().await.unwrap().map_err(GetError::new)
            }
            Some(CacheEntry::Node(ref mut node)) => {
                let real_node = node.unwrap_mut();
                real_node.bump_access_time();
                Ok(real_node.value.clone())
            }
            Some(CacheEntry::FetchFailed(e)) => Err(GetError::new(e.clone())),
        }
    }

    pub async fn insert(&self, k: K, v: Arc<V>) {
        self.data
            .lock()
            .await
            .insert(k, CacheEntry::Node(CacheNode::new(v)));
    }

    pub async fn remove(&self, k: K) {
        self.data.lock().await.remove(&k);
    }

    // Evicts the given key from the cache if the value's ref count is 1.
    pub async fn try_evict(&self, k: K) -> bool {
        let data = self.data.clone();
        let mut lock = data.lock().await;
        self.try_evict_with_lock(k, &mut lock).await
    }

    // Evicts all keys from the cache, waiting if necessary until all ref counts are 1.
    pub async fn evict_all_sync(&mut self) {
        // Make sure to hold the lock until the end of the function.
        let mut data = self.data.lock().await;
        loop {
            let keys: Vec<_> = data.keys().copied().collect();
            if keys.is_empty() {
                break;
            }

            let mut all_done = true;
            for key in keys {
                all_done = all_done && self.try_evict_with_lock(key, &mut data).await;
            }

            if all_done {
                break;
            }

            sleep(Duration::from_secs(1)).await;
        }
    }

    // Returns false if the key can't be evicted because the reference
    // count of the Arc is not one.
    async fn try_evict_with_lock(
        &self,
        k: K,
        lock: &mut tokio::sync::MutexGuard<'_, HashMap<K, CacheEntry<V>>>,
    ) -> bool {
        let store = self.store.clone();
        match lock.entry(k) {
            hash_map::Entry::Vacant(_) => true,
            hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                CacheEntry::Fetching(_) | CacheEntry::FetchFailed(_) => {
                    e.remove();
                    true
                }
                CacheEntry::Node(node) => match mem::replace(node, CacheNode::Dummy) {
                    CacheNode::Real(real_node) => match RealCacheNode::try_unwrap(real_node) {
                        Ok(v) => {
                            e.remove();
                            store.update(k, v).await;
                            true
                        }
                        Err(real_node) => {
                            // If the unwrap wasn't successful, replace the dummy cache node
                            // with the real cache node.
                            *node = CacheNode::Real(real_node);
                            false
                        }
                    },
                    CacheNode::Dummy => false,
                },
            },
        }
    }

    fn pruner_join_handle(
        data: Arc<Mutex<HashMap<K, CacheEntry<V>>>>,
        store: Arc<dyn Store<K, V> + Send + Sync>,
        ttl: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                // iterate over all entries. if CacheEntry::Value
                // and arc count is 1 (and idle for long time) then
                // evict
                let mut data = data.lock().await;
                let keys: Vec<_> = data.keys().copied().collect();
                let now = Instant::now();
                for key in keys {
                    let entry = data.entry(key);
                    if let hash_map::Entry::Occupied(mut e) = entry {
                        if let CacheEntry::Node(ref mut node) = e.get_mut() {
                            if now.duration_since(node.unwrap().last_access_ts) < ttl {
                                continue;
                            }
                            match mem::replace(node, CacheNode::Dummy) {
                                CacheNode::Real(real_node) => {
                                    match RealCacheNode::try_unwrap(real_node) {
                                        Ok(v) => {
                                            e.remove();
                                            store.update(key, v).await;
                                        }
                                        Err(real_node) => {
                                            *node = CacheNode::Real(real_node);
                                        }
                                    }
                                }
                                CacheNode::Dummy => (),
                            }
                        }
                    }
                }
                drop(data);
                sleep(Duration::from_secs(10)).await;
            }
        })
    }
}

impl<K, V> Drop for Cache<K, V> {
    fn drop(&mut self) {
        self.pruner_join_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;
    use tokio::task::JoinSet;
    use tokio::time::{sleep, Duration};

    #[derive(Debug, PartialEq, Eq)]
    enum StoreOperation {
        Fetch(i32),
        Update((i32, String)),
    }

    struct TestStore {
        tx: mpsc::UnboundedSender<StoreOperation>,
    }

    #[async_trait]
    impl Store<i32, String> for TestStore {
        async fn fetch(&self, key: &i32) -> anyhow::Result<String> {
            self.tx.send(StoreOperation::Fetch(*key)).unwrap();
            Ok(String::from("Hello"))
        }

        async fn update(&self, key: i32, value: String) {
            self.tx.send(StoreOperation::Update((key, value))).unwrap();
        }
    }

    #[tokio::test]
    async fn it_works() {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut cache = Cache::new(TestStore { tx }, Duration::from_secs(60)).await;

        {
            let v = cache.get(10).await.unwrap();
            assert_eq!("Hello", *v);
            let v = cache.get(10).await.unwrap();
            assert_eq!("Hello", *v);
        }

        cache.evict_all_sync().await;

        drop(cache);

        tokio::spawn(async move {
            let mut operations = vec![];
            while let Some(op) = rx.recv().await {
                operations.push(op);
            }
            assert_eq!(
                vec![
                    StoreOperation::Fetch(10),
                    StoreOperation::Update((10, "Hello".to_string()))
                ],
                operations
            );
        })
        .await
        .unwrap();
    }

    struct StoreWithLatency;

    #[async_trait]
    impl Store<i32, String> for StoreWithLatency {
        async fn fetch(&self, _key: &i32) -> anyhow::Result<String> {
            sleep(Duration::from_secs(1)).await;
            Ok(String::from("Hello"))
        }

        async fn update(&self, _key: i32, _value: String) {
            sleep(Duration::from_secs(1)).await;
        }
    }

    #[tokio::test]
    async fn multiple_waiters() {
        let cache = Arc::new(Cache::new(StoreWithLatency, Duration::from_secs(60)).await);

        let mut tasks = JoinSet::new();
        for _ in 1..100 {
            let cache = cache.clone();
            tasks.spawn(async move {
                let v = cache.get(1).await.unwrap();
                assert_eq!("Hello", *v);
            });
        }

        while let Some(res) = tasks.join_next().await {
            assert!(res.is_ok());
        }
    }
}
