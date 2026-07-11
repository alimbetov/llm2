use astravector_runtime::{
    config::{BatchProfile, BatchingConfig, InferenceRetryConfig},
    error::AstraError,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
    scheduler::{QueueKind, Scheduler},
};
use async_trait::async_trait;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

struct ControlledSlowEngine {
    delay_ms: AtomicU64,
}

#[async_trait]
impl InferenceEngine for ControlledSlowEngine {
    async fn encode_batch(
        &self,
        inputs: Vec<InferenceInput>,
    ) -> Result<Vec<EmbeddingResult>, AstraError> {
        tokio::time::sleep(Duration::from_millis(self.delay_ms.load(Ordering::SeqCst))).await;
        Ok(inputs
            .into_iter()
            .map(|_| EmbeddingResult {
                dense: Some(vec![1.0; 8]),
                sparse_indices: None,
                sparse_values: None,
                token_count: 1,
                truncated: false,
            })
            .collect())
    }

    fn dense_available(&self) -> bool {
        true
    }
    fn sparse_available(&self) -> bool {
        false
    }
    fn count_tokens(
        &self,
        _text: &str,
        _max_length: usize,
        _allow_truncation: bool,
    ) -> Result<usize, AstraError> {
        Ok(1)
    }
    async fn self_test(&self) -> Result<(), AstraError> {
        Ok(())
    }
}

fn input() -> InferenceInput {
    InferenceInput {
        text: "controlled recovery".into(),
        max_length: 16,
        allow_truncation: false,
        want_dense: true,
        want_sparse: false,
        token_count_hint: 1,
    }
}

fn profile(capacity: usize) -> BatchProfile {
    BatchProfile {
        max_wait_ms: 1,
        max_items: 1,
        max_padded_tokens: 16,
        queue_capacity: capacity,
        max_queue_age_ms: 25,
        min_inference_budget_ms: 25,
        max_deadline_skew_ms: 50,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controlled_spike_sheds_load_and_recovers_without_stale_backlog() {
    let engine = Arc::new(ControlledSlowEngine {
        delay_ms: AtomicU64::new(100),
    });
    let scheduler = Scheduler::start(
        engine.clone(),
        BatchingConfig {
            query: profile(8),
            document: profile(32),
            buckets: vec![16],
        },
        8,
        InferenceRetryConfig::default(),
    );

    let mut tasks = Vec::new();
    for _ in 0..32 {
        let scheduler = scheduler.clone();
        tasks.push(tokio::spawn(async move {
            scheduler
                .submit(
                    QueueKind::Query,
                    input(),
                    Instant::now() + Duration::from_millis(500),
                    CancellationToken::new(),
                )
                .await
        }));
    }
    let mut shed = 0;
    for task in tasks {
        if matches!(task.await.unwrap(), Err(AstraError::ResourceExhausted(_))) {
            shed += 1;
        }
    }
    assert!(shed > 0, "controlled spike must shed overload early");

    engine.delay_ms.store(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let started = Instant::now();
    let fresh = scheduler
        .submit(
            QueueKind::Query,
            input(),
            Instant::now() + Duration::from_millis(250),
            CancellationToken::new(),
        )
        .await;
    assert!(
        fresh.is_ok(),
        "fresh request must not inherit stale backlog"
    );
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "scheduler did not recover within integration threshold"
    );
}
