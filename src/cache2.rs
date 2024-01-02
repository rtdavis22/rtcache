use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Default)]
pub struct StringCache {
    cache: HashMap<i32, Arc<RwLock<String>>>,
}

impl StringCache {
    fn get(&self, key: i32) -> Arc<RwLock<String>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn it_works() {
        let cache = StringCache::default();
    }
}
