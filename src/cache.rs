// TODO:
// 1. Replace Mutex<String> with T
// 2. Support Fetchers and Stores
// 3. Add examples and unit tests
// 4. Docs
// 5. Support other common functionality.
// 6. Clean up join handle stuff.
// 7. Support ttl/access ttl (see moka)

use std::collections::{hash_map, HashMap};
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{sleep, Duration};

#[derive(Debug)]
pub enum CacheEntry {
    Fetching(broadcast::Sender<Result<Arc<Mutex<String>>, FetchError>>),
    Value(Arc<Mutex<String>>),
}

#[derive(Debug)]
pub struct Cache {
    data: Arc<Mutex<HashMap<i32, CacheEntry>>>,
    evict_tx: mpsc::UnboundedSender<Mutex<String>>,
    evictor_join_handle: tokio::task::JoinHandle<()>,
    pruner_join_handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub enum FetchError {
    NotFound,
}

impl Cache {
    pub async fn new() -> Self {
        let data = Arc::new(Mutex::new(HashMap::new()));

        let (evict_tx, evict_rx) = mpsc::unbounded_channel();

        let evictor_join_handle = Self::evictor_join_handle(evict_rx);

        let pruner_join_handle = Self::pruner_join_handle(data.clone(), evict_tx.clone());

        Self {
            data,
            evict_tx,
            evictor_join_handle,
            pruner_join_handle,
        }
    }

    pub async fn get(&self, k: i32) -> Result<Arc<Mutex<String>>, FetchError> {
        let data = self.data.clone();
        let mut lock = self.data.lock().await;

        match lock.get(&k) {
            None => {
                let (tx, mut rx) = broadcast::channel(1);
                lock.insert(k, CacheEntry::Fetching(tx.clone()));
                drop(lock);

                tokio::spawn(async move {
                    let fetch_result = if k % 2 == 0 {
                        Ok(Arc::new(Mutex::new(String::from("Hello"))))
                    } else {
                        Err(FetchError::NotFound)
                    };

                    let mut data = data.lock().await;
                    let result = match data.entry(k) {
                        hash_map::Entry::Occupied(mut e) => match e.get() {
                            // This could mean that the key was inserted while the
                            // fetch was happening. In this case, we ignore the fetched
                            // value and return the inserted value.
                            CacheEntry::Value(arc) => Ok(arc.clone()),
                            CacheEntry::Fetching(_) => {
                                if let Ok(res) = fetch_result.clone() {
                                    e.insert(CacheEntry::Value(res));
                                } else {
                                    e.remove();
                                }
                                fetch_result
                            }
                        },
                        // This can happen if the value in the cache was deleted while
                        // the fetch was happening.
                        hash_map::Entry::Vacant(_) => Err(FetchError::NotFound),
                    };
                    drop(data);

                    let _ = tx.send(result);
                });

                rx.recv().await.unwrap()
            }
            Some(CacheEntry::Fetching(tx)) => tx.subscribe().recv().await.unwrap(),
            Some(CacheEntry::Value(v)) => Ok(v.clone()),
        }
    }

    pub async fn insert(&self, k: i32, v: Arc<Mutex<String>>) {
        self.data
            .clone()
            .lock()
            .await
            .insert(k, CacheEntry::Value(v));
    }

    // Returns false if the key can't be evicted because the reference
    // count of the Arc is not one.
    async fn try_evict_without_lock(
        &self,
        k: i32,
        lock: &mut tokio::sync::MutexGuard<'_, HashMap<i32, CacheEntry>>,
    ) -> bool {
        //let data = self.data.clone();
        //let mut lock = data.lock().await;

        match lock.entry(k) {
            hash_map::Entry::Vacant(_) => true,
            hash_map::Entry::Occupied(e) => match e.get() {
                CacheEntry::Fetching(_) => {
                    e.remove();
                    true
                }
                CacheEntry::Value(_) => {
                    let (k, v) = e.remove_entry();
                    if let CacheEntry::Value(v) = v {
                        match Arc::try_unwrap(v) {
                            Ok(v) => {
                                self.evict_tx.send(v).unwrap();
                                true
                            }
                            Err(arc) => {
                                lock.insert(k, CacheEntry::Value(arc));
                                false
                            }
                        }
                    } else {
                        true
                    }
                }
            },
        }
    }

    pub async fn try_evict(&self, k: i32) -> bool {
        let data = self.data.clone();
        let mut lock = data.lock().await;
        self.try_evict_without_lock(k, &mut lock).await
    }

    pub async fn evict_all_sync(&mut self) {
        let data_clone = self.data.clone();

        // Make sure to hold the lock until the end of the function.
        let mut data = self.data.lock().await;
        loop {
            println!("Cache: {:#?}", self);

            let keys: Vec<_> = data.keys().copied().collect();
            if keys.is_empty() {
                break;
            }

            for key in keys {
                self.try_evict_without_lock(key, &mut data).await;
            }

            sleep(Duration::from_secs(1)).await;
        }

        // At this point, the cache is empty and we need to wait for the evictor
        // to finish. To do this, we construct a new evictor_join_handle
        // and .await on the old one. This requires constructing a new channel
        // and a new pruner_join_handle.

        let (new_evict_tx, new_evict_rx) = mpsc::unbounded_channel();
        let new_pruner_join_handle = Self::pruner_join_handle(data_clone, new_evict_tx.clone());

        drop(std::mem::replace(&mut self.evict_tx, new_evict_tx));

        let new_evictor_join_handle = Self::evictor_join_handle(new_evict_rx);

        // Abort the old pruner so its evict_tx is dropped,
        // allowing the old evictor to complete.
        self.pruner_join_handle.abort();
        self.pruner_join_handle = new_pruner_join_handle;

        // Replace the evictor and wait for the old evictor to evict everything.
        std::mem::replace(&mut self.evictor_join_handle, new_evictor_join_handle)
            .await
            .unwrap();
    }

    pub fn evictor_join_handle(
        mut rx: mpsc::UnboundedReceiver<Mutex<String>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(v) = rx.recv().await {
                println!("Evicting {:#?}\n", v);
            }
        })
    }

    fn pruner_join_handle(
        data: Arc<Mutex<HashMap<i32, CacheEntry>>>,
        tx: mpsc::UnboundedSender<Mutex<String>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                // iterate over all entries. if CacheEntry::Value
                // and arc count is 1 (and idle for long time) then
                // evict
                let mut data = data.lock().await;
                let keys: Vec<_> = data.keys().copied().collect();
                for key in keys {
                    let entry = data.entry(key);
                    if let hash_map::Entry::Occupied(e) = entry {
                        if matches!(e.get(), CacheEntry::Value(_)) {
                            let (k, v) = e.remove_entry();
                            if let CacheEntry::Value(v) = v {
                                match Arc::try_unwrap(v) {
                                    Ok(v) => {
                                        tx.send(v).unwrap();
                                    }
                                    Err(arc) => {
                                        data.insert(k, CacheEntry::Value(arc));
                                    }
                                };
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
