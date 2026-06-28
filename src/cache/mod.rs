use crate::inference::EmbeddingResult;
use moka::future::Cache;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub struct CachedEmbedding {
    pub result: Arc<EmbeddingResult>,
    pub persisted_in_postgres: bool,
}

#[derive(Clone)]
pub struct L1Cache {
    enabled: bool,
    inner: Arc<Cache<String, Arc<CachedEmbedding>>>,
}
impl L1Cache {
    pub fn new(enabled: bool, max_entries: u64, ttl_minutes: u64, idle_minutes: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(Duration::from_secs(ttl_minutes * 60))
            .time_to_idle(Duration::from_secs(idle_minutes * 60))
            .build();
        Self {
            enabled,
            inner: Arc::new(cache),
        }
    }
    pub async fn get(&self, key: &str) -> Option<Arc<CachedEmbedding>> {
        if self.enabled {
            self.inner.get(key).await
        } else {
            None
        }
    }
    pub async fn put(&self, key: String, value: Arc<EmbeddingResult>, persisted_in_postgres: bool) {
        if self.enabled {
            self.inner
                .insert(
                    key,
                    Arc::new(CachedEmbedding {
                        result: value,
                        persisted_in_postgres,
                    }),
                )
                .await
        }
    }
}
