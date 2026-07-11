use crate::{
    config::{BatchingConfig, InferenceRetryConfig, RetryPolicyConfig},
    error::AstraError,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
};
use futures::{stream, StreamExt};
use metrics::{counter, gauge, histogram};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Query,
    Document,
}

struct Job {
    input: InferenceInput,
    enqueued_at: Instant,
    deadline: Instant,
    cancel: CancellationToken,
    tx: oneshot::Sender<Result<EmbeddingResult, AstraError>>,
}

#[derive(Clone)]
pub struct Scheduler {
    query_tx: mpsc::Sender<Job>,
    document_tx: mpsc::Sender<Job>,
    healthy: watch::Receiver<bool>,
}

#[derive(Debug, Clone)]
pub struct SubmitManyOptions {
    pub max_in_flight: usize,
    pub preserve_order: bool,
    pub cancel_on_error: bool,
}

impl Default for SubmitManyOptions {
    fn default() -> Self {
        Self {
            max_in_flight: 32,
            preserve_order: true,
            cancel_on_error: true,
        }
    }
}

impl Scheduler {
    pub fn start(
        engine: Arc<dyn InferenceEngine>,
        cfg: BatchingConfig,
        max_consecutive_queries: usize,
        inference_retry: InferenceRetryConfig,
    ) -> Self {
        let (query_tx, query_rx) = mpsc::channel(cfg.query.queue_capacity);
        let (document_tx, document_rx) = mpsc::channel(cfg.document.queue_capacity);
        let (health_tx, healthy) = watch::channel(true);
        tokio::spawn(async move {
            let ok = run_loop(
                engine,
                query_rx,
                document_rx,
                cfg,
                max_consecutive_queries,
                inference_retry,
            )
            .await
            .is_ok();
            let _ = health_tx.send(ok);
        });
        Self {
            query_tx,
            document_tx,
            healthy,
        }
    }

    pub fn healthy(&self) -> bool {
        *self.healthy.borrow()
    }

    pub async fn submit(
        &self,
        kind: QueueKind,
        input: InferenceInput,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> Result<EmbeddingResult, AstraError> {
        if Instant::now() >= deadline {
            return Err(AstraError::DeadlineExceeded("deadline before queue".into()));
        }
        let (tx, rx) = oneshot::channel();
        let sender = match kind {
            QueueKind::Query => &self.query_tx,
            QueueKind::Document => &self.document_tx,
        };
        let queue_label = queue_label(kind);
        gauge!("astravector_queue_depth", "queue" => queue_label).increment(1.0);
        sender
            .try_send(Job {
                input,
                enqueued_at: Instant::now(),
                deadline,
                cancel: cancel.clone(),
                tx,
            })
            .map_err(|error| {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                match error {
                    mpsc::error::TrySendError::Full(_) => {
                        counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "full").increment(1);
                        AstraError::ResourceExhausted(format!("embedding {queue_label} queue full"))
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "closed").increment(1);
                        AstraError::Unavailable("embedding scheduler stopped".into())
                    }
                }
            })?;

        tokio::select! {
            _ = cancel.cancelled() => Err(AstraError::Cancelled("request cancelled".into())),
            result = tokio::time::timeout_at(deadline, rx) => match result {
                Ok(Ok(value)) => value,
                Ok(Err(_)) => Err(AstraError::Unavailable("scheduler stopped".into())),
                Err(_) => Err(AstraError::DeadlineExceeded("scheduler deadline".into())),
            }
        }
    }

    pub async fn submit_many(
        &self,
        kind: QueueKind,
        inputs: Vec<InferenceInput>,
        deadline: Instant,
        cancel: CancellationToken,
        options: SubmitManyOptions,
    ) -> Result<Vec<EmbeddingResult>, AstraError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let max_in_flight = options.max_in_flight.max(1);
        let group_cancel = if options.cancel_on_error {
            cancel.child_token()
        } else {
            cancel.clone()
        };
        counter!("embedding_document_submit_many_total").increment(1);
        gauge!("embedding_document_max_in_flight_current").set(max_in_flight as f64);
        let started = Instant::now();
        let scheduler = self.clone();
        let total_inputs = inputs.len();
        let mut stream = stream::iter(inputs.into_iter().enumerate())
            .map(|(idx, input)| {
                let scheduler = scheduler.clone();
                let cancel = group_cancel.clone();
                async move {
                    let result = scheduler.submit(kind, input, deadline, cancel).await;
                    (idx, result)
                }
            })
            .buffer_unordered(max_in_flight);
        let mut outputs = Vec::with_capacity(total_inputs);
        while let Some((idx, item)) = stream.next().await {
            match item {
                Ok(value) => outputs.push((idx, value)),
                Err(err) if options.cancel_on_error => {
                    group_cancel.cancel();
                    let skipped = total_inputs.saturating_sub(outputs.len() + 1);
                    counter!("embedding_document_chunks_cancelled_total").increment(skipped as u64);
                    counter!("embedding_document_chunks_skipped_after_error_total")
                        .increment(skipped as u64);
                    counter!("embedding_document_submit_many_failed_total").increment(1);
                    return Err(err);
                }
                Err(err) => {
                    counter!("embedding_document_submit_many_failed_total").increment(1);
                    return Err(err);
                }
            }
        }
        if options.preserve_order {
            outputs.sort_by_key(|(idx, _)| *idx);
        }
        let result = outputs
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        counter!("embedding_document_chunks_completed_total").increment(result.len() as u64);
        histogram!("embedding_document_submit_many_duration_ms")
            .record(started.elapsed().as_millis() as f64);
        Ok(result)
    }
}

async fn run_loop(
    engine: Arc<dyn InferenceEngine>,
    mut query_rx: mpsc::Receiver<Job>,
    mut document_rx: mpsc::Receiver<Job>,
    cfg: BatchingConfig,
    max_consecutive_queries: usize,
    inference_retry: InferenceRetryConfig,
) -> Result<(), AstraError> {
    let mut pending_query = VecDeque::new();
    let mut pending_document = VecDeque::new();
    let mut consecutive_query_batches = 0usize;

    loop {
        while let Ok(job) = query_rx.try_recv() {
            pending_query.push_back(job);
        }
        while let Ok(job) = document_rx.try_recv() {
            pending_document.push_back(job);
        }

        if pending_query.is_empty() && pending_document.is_empty() {
            tokio::select! {
                biased;
                job = query_rx.recv() => {
                    match job {
                        Some(job) => pending_query.push_back(job),
                        None if document_rx.is_closed() => break,
                        None => {}
                    }
                }
                job = document_rx.recv() => {
                    match job {
                        Some(job) => pending_document.push_back(job),
                        None if query_rx.is_closed() => break,
                        None => {}
                    }
                }
            }
            continue;
        }

        let kind = if !pending_query.is_empty()
            && (consecutive_query_batches < max_consecutive_queries || pending_document.is_empty())
        {
            QueueKind::Query
        } else {
            QueueKind::Document
        };

        if kind == QueueKind::Query {
            consecutive_query_batches += 1;
        } else {
            consecutive_query_batches = 0;
        }

        let (profile, queue) = match kind {
            QueueKind::Query => (&cfg.query, &mut pending_query),
            QueueKind::Document => (&cfg.document, &mut pending_document),
        };
        let queue_label = queue_label(kind);
        if let Some(oldest) = queue.front() {
            gauge!("astravector_queue_oldest_age_seconds", "queue" => queue_label)
                .set(oldest.enqueued_at.elapsed().as_secs_f64());
        } else {
            gauge!("astravector_queue_oldest_age_seconds", "queue" => queue_label).set(0.0);
        }
        let started = Instant::now();
        let mut jobs: Vec<Job> = Vec::new();
        let mut target_bucket = None;
        let mut rotations = 0usize;

        while jobs.len() < profile.max_items
            && started.elapsed() < Duration::from_millis(profile.max_wait_ms)
        {
            let Some(job) = queue.pop_front() else {
                tokio::time::sleep(Duration::from_micros(200)).await;
                while let Ok(job) = match kind {
                    QueueKind::Query => query_rx.try_recv(),
                    QueueKind::Document => document_rx.try_recv(),
                } {
                    queue.push_back(job);
                }
                continue;
            };

            let now = Instant::now();
            let queue_age = now.saturating_duration_since(job.enqueued_at);
            histogram!("astravector_queue_wait_seconds", "queue" => queue_label)
                .record(queue_age.as_secs_f64());
            if job.cancel.is_cancelled() {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "cancelled").increment(1);
                let _ = job.tx.send(Err(AstraError::Cancelled(
                    "request cancelled in queue".into(),
                )));
                continue;
            }
            if now >= job.deadline {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "deadline_expired").increment(1);
                counter!("astravector_deadline_rejected_total", "stage" => "inference_queue", "reason" => "deadline_expired").increment(1);
                let _ = job.tx.send(Err(AstraError::DeadlineExceeded(
                    "deadline_expired_in_queue".into(),
                )));
                continue;
            }
            if kind == QueueKind::Query
                && profile.max_queue_age_ms > 0
                && queue_age > Duration::from_millis(profile.max_queue_age_ms)
            {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "age_exceeded").increment(1);
                let _ = job.tx.send(Err(AstraError::ResourceExhausted(
                    "queue_age_exceeded".into(),
                )));
                continue;
            }
            let remaining = job.deadline.saturating_duration_since(now);
            histogram!("astravector_deadline_remaining_seconds", "stage" => "inference_queue")
                .record(remaining.as_secs_f64());
            if profile.min_inference_budget_ms > 0
                && remaining < Duration::from_millis(profile.min_inference_budget_ms)
            {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                counter!("astravector_queue_rejected_total", "queue" => queue_label, "reason" => "insufficient_budget").increment(1);
                counter!("astravector_deadline_rejected_total", "stage" => "inference_queue", "reason" => "insufficient_budget").increment(1);
                let _ = job.tx.send(Err(AstraError::DeadlineExceeded(
                    "insufficient_inference_budget".into(),
                )));
                continue;
            }

            if let Some(first) = jobs.first() {
                let first_remaining = first.deadline.saturating_duration_since(now);
                let skew = first_remaining.abs_diff(remaining);
                histogram!("astravector_batch_deadline_skew_seconds", "queue" => queue_label)
                    .record(skew.as_secs_f64());
                if profile.max_deadline_skew_ms > 0
                    && skew > Duration::from_millis(profile.max_deadline_skew_ms)
                {
                    queue.push_back(job);
                    rotations += 1;
                    if rotations > queue.len() {
                        break;
                    }
                    continue;
                }
            }

            let bucket = bucket_for(job.input.token_count_hint, &cfg.buckets);
            match target_bucket {
                None => target_bucket = Some(bucket),
                Some(target) if target != bucket && rotations <= queue.len() => {
                    rotations += 1;
                    queue.push_back(job);
                    continue;
                }
                _ => {}
            }

            let candidate_max = jobs
                .iter()
                .map(|existing: &Job| existing.input.token_count_hint)
                .chain(std::iter::once(job.input.token_count_hint))
                .max()
                .unwrap_or(0);
            if !jobs.is_empty() && (jobs.len() + 1) * candidate_max > profile.max_padded_tokens {
                queue.push_front(job);
                break;
            }
            jobs.push(job);
            gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
        }

        if jobs.is_empty() {
            continue;
        }

        histogram!("astravector_batch_size", "queue" => queue_label).record(jobs.len() as f64);

        let Some(earliest_deadline) = jobs.iter().map(|job| job.deadline).min() else {
            continue;
        };
        if Instant::now() >= earliest_deadline {
            for job in jobs {
                let _ = job
                    .tx
                    .send(Err(AstraError::DeadlineExceeded("batch deadline".into())));
            }
            continue;
        }

        let before_cancel_filter = jobs.len();
        jobs.retain(|job| !job.cancel.is_cancelled());
        let cancelled_before_inference = before_cancel_filter.saturating_sub(jobs.len());
        if cancelled_before_inference > 0 {
            counter!("embedding_document_chunks_cancelled_total")
                .increment(cancelled_before_inference as u64);
        }
        if jobs.is_empty() {
            continue;
        }

        let inputs = jobs.iter().map(|job| job.input.clone()).collect();
        let inference_started = std::time::Instant::now();
        let retry_policy = match kind {
            QueueKind::Query => &inference_retry.query,
            QueueKind::Document => &inference_retry.document,
        };
        let result = encode_batch_with_retry(
            engine.clone(),
            inputs,
            retry_policy,
            earliest_deadline,
            queue_label,
        )
        .await;
        histogram!("astravector_inference_duration_seconds")
            .record(inference_started.elapsed().as_secs_f64());

        match result {
            Ok(results) if results.len() == jobs.len() => {
                for (job, result) in jobs.into_iter().zip(results) {
                    let _ = job.tx.send(Ok(result));
                }
            }
            Ok(_) => {
                for job in jobs {
                    let _ = job.tx.send(Err(AstraError::Internal(
                        "engine result count mismatch".into(),
                    )));
                }
            }
            Err(error) => {
                for job in jobs {
                    let _ = job.tx.send(Err(error.clone()));
                }
            }
        }
    }
    Ok(())
}

async fn encode_batch_with_retry(
    engine: Arc<dyn InferenceEngine>,
    inputs: Vec<InferenceInput>,
    policy: &RetryPolicyConfig,
    deadline: Instant,
    workload: &'static str,
) -> Result<Vec<EmbeddingResult>, AstraError> {
    let max_attempts = if policy.enabled {
        policy.max_attempts.max(1)
    } else {
        1
    };
    let mut attempt = 1u32;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AstraError::DeadlineExceeded("inference deadline".into()));
        }
        let result = tokio::time::timeout(remaining, engine.encode_batch(inputs.clone())).await;
        match result {
            Ok(Ok(outputs)) => {
                if attempt > 1 {
                    counter!("astravector_retry_success_total", "operation" => "encode_batch", "workload" => workload).increment(1);
                }
                return Ok(outputs);
            }
            Ok(Err(err))
                if is_retryable_inference_error(&err, policy) && attempt < max_attempts =>
            {
                let delay = retry_delay(policy, attempt);
                if !retry_budget_allows(policy, deadline, delay) {
                    counter!("astravector_retry_skipped_total", "operation" => "encode_batch", "workload" => workload, "reason" => "insufficient_budget").increment(1);
                    return Err(err);
                }
                counter!("astravector_retry_attempts_total", "operation" => "encode_batch", "workload" => workload).increment(1);
                histogram!("inference_retry_delay_ms", "operation" => "encode_batch")
                    .record(delay.as_millis() as f64);
                tracing::warn!(attempt, error=%err, "inference encode_batch transient failure; retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Ok(Err(err)) if is_retryable_inference_error(&err, policy) => {
                counter!("astravector_retry_exhausted_total", "operation" => "encode_batch", "workload" => workload)
                    .increment(1);
                return Err(err);
            }
            Ok(Err(err)) => {
                counter!("astravector_retry_skipped_total", "operation" => "encode_batch", "workload" => workload, "reason" => retry_skip_reason(&err)).increment(1);
                return Err(err);
            }
            Err(_) if policy.retry_on_timeout && attempt < max_attempts => {
                let delay = retry_delay(policy, attempt);
                if !retry_budget_allows(policy, deadline, delay) {
                    counter!("astravector_retry_skipped_total", "operation" => "encode_batch", "workload" => workload, "reason" => "insufficient_budget").increment(1);
                    return Err(AstraError::DeadlineExceeded("inference deadline".into()));
                }
                counter!("astravector_retry_attempts_total", "operation" => "encode_batch", "workload" => workload).increment(1);
                histogram!("inference_retry_delay_ms", "operation" => "encode_batch")
                    .record(delay.as_millis() as f64);
                tracing::warn!(attempt, "inference encode_batch timeout; retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(_) => {
                counter!("astravector_retry_exhausted_total", "operation" => "encode_batch", "workload" => workload)
                    .increment(1);
                return Err(AstraError::DeadlineExceeded("inference deadline".into()));
            }
        }
    }
}

fn retry_budget_allows(policy: &RetryPolicyConfig, deadline: Instant, delay: Duration) -> bool {
    let required = delay
        .saturating_add(Duration::from_millis(policy.min_remaining_budget_ms))
        .saturating_add(Duration::from_millis(25));
    deadline.saturating_duration_since(Instant::now()) >= required
}

fn retry_skip_reason(error: &AstraError) -> &'static str {
    match error {
        AstraError::ResourceExhausted(_) => "overload",
        AstraError::Cancelled(_) => "cancelled",
        _ => "non_retryable_error",
    }
}

fn is_retryable_inference_error(err: &AstraError, policy: &RetryPolicyConfig) -> bool {
    match err {
        AstraError::DeadlineExceeded(_) => policy.retry_on_timeout,
        AstraError::Unavailable(_) => policy.retry_on_unavailable,
        _ => false,
    }
}

fn retry_delay(policy: &RetryPolicyConfig, attempt: u32) -> Duration {
    let exp = 1u64
        .checked_shl(attempt.saturating_sub(1))
        .unwrap_or(u64::MAX);
    let base = policy
        .base_delay_ms
        .saturating_mul(exp)
        .min(policy.max_delay_ms);
    let jitter = if policy.jitter_enabled {
        (attempt as u64 * 17) % 31
    } else {
        0
    };
    Duration::from_millis(base.saturating_add(jitter))
}

fn bucket_for(tokens: usize, buckets: &[usize]) -> usize {
    buckets
        .iter()
        .copied()
        .find(|bucket| tokens <= *bucket)
        .unwrap_or_else(|| buckets.last().copied().unwrap_or(tokens))
}

fn queue_label(kind: QueueKind) -> &'static str {
    match kind {
        QueueKind::Query => "query",
        QueueKind::Document => "document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BatchProfile, InferenceRetryConfig};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    struct TestEngine {
        delay: Duration,
        outcomes: Mutex<VecDeque<Result<(), AstraError>>>,
        calls: AtomicUsize,
    }

    impl TestEngine {
        fn fixed(delay: Duration) -> Self {
            Self {
                delay,
                outcomes: Mutex::new(VecDeque::new()),
                calls: AtomicUsize::new(0),
            }
        }

        fn with_outcomes(outcomes: Vec<Result<(), AstraError>>) -> Self {
            Self {
                delay: Duration::ZERO,
                outcomes: Mutex::new(outcomes.into()),
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl InferenceEngine for TestEngine {
        async fn encode_batch(
            &self,
            inputs: Vec<InferenceInput>,
        ) -> Result<Vec<EmbeddingResult>, AstraError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            if let Some(outcome) = self.outcomes.lock().await.pop_front() {
                outcome?;
            }
            Ok(inputs.into_iter().map(|_| embedding()).collect())
        }

        fn dense_available(&self) -> bool {
            true
        }
        fn sparse_available(&self) -> bool {
            true
        }
        fn count_tokens(
            &self,
            text: &str,
            _max_length: usize,
            _allow_truncation: bool,
        ) -> Result<usize, AstraError> {
            Ok(text.split_whitespace().count().max(1))
        }
        async fn self_test(&self) -> Result<(), AstraError> {
            Ok(())
        }
    }

    fn embedding() -> EmbeddingResult {
        EmbeddingResult {
            dense: Some(vec![1.0; 4]),
            sparse_indices: None,
            sparse_values: None,
            token_count: 1,
            truncated: false,
        }
    }

    fn input() -> InferenceInput {
        InferenceInput {
            text: "recovery".into(),
            max_length: 32,
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
            max_padded_tokens: 32,
            queue_capacity: capacity,
            max_queue_age_ms: 250,
            min_inference_budget_ms: 50,
            max_deadline_skew_ms: 250,
        }
    }

    fn scheduler(engine: Arc<TestEngine>, query: BatchProfile) -> Scheduler {
        Scheduler::start(
            engine,
            BatchingConfig {
                query,
                document: profile(8),
                buckets: vec![32],
            },
            8,
            InferenceRetryConfig::default(),
        )
    }

    #[tokio::test]
    async fn query_queue_full_returns_resource_exhausted() {
        let (tx, _rx) = mpsc::channel(1);
        let (_health_tx, healthy) = watch::channel(true);
        let scheduler = Scheduler {
            query_tx: tx.clone(),
            document_tx: tx,
            healthy,
        };
        let first = tokio::spawn({
            let scheduler = scheduler.clone();
            async move {
                scheduler
                    .submit(
                        QueueKind::Query,
                        input(),
                        Instant::now() + Duration::from_secs(2),
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        tokio::task::yield_now().await;
        let error = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, AstraError::ResourceExhausted(message) if message.contains("query queue full"))
        );
        first.abort();
    }

    #[tokio::test]
    async fn closed_scheduler_returns_unavailable() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let (_health_tx, healthy) = watch::channel(false);
        let scheduler = Scheduler {
            query_tx: tx.clone(),
            document_tx: tx,
            healthy,
        };
        let error = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, AstraError::Unavailable(message) if message.contains("scheduler stopped"))
        );
    }

    #[tokio::test]
    async fn expired_job_returns_deadline_exceeded() {
        let scheduler = scheduler(Arc::new(TestEngine::fixed(Duration::ZERO)), profile(2));
        let error = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now(),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, AstraError::DeadlineExceeded(_)));
    }

    #[tokio::test]
    async fn old_query_job_returns_resource_exhausted() {
        let mut query = profile(4);
        query.max_queue_age_ms = 10;
        let scheduler = scheduler(
            Arc::new(TestEngine::fixed(Duration::from_millis(50))),
            query,
        );
        let first = tokio::spawn({
            let scheduler = scheduler.clone();
            async move {
                scheduler
                    .submit(
                        QueueKind::Query,
                        input(),
                        Instant::now() + Duration::from_secs(1),
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        tokio::time::sleep(Duration::from_millis(2)).await;
        let error = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, AstraError::ResourceExhausted(message) if message == "queue_age_exceeded")
        );
        assert!(first.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn insufficient_budget_skips_inference() {
        let engine = Arc::new(TestEngine::fixed(Duration::ZERO));
        let mut query = profile(2);
        query.min_inference_budget_ms = 200;
        let scheduler = scheduler(engine.clone(), query);
        let error = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_millis(100),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, AstraError::DeadlineExceeded(message) if message == "insufficient_inference_budget")
        );
        assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn stale_job_does_not_poison_fresh_batch() {
        let engine = Arc::new(TestEngine::fixed(Duration::ZERO));
        let mut query = profile(4);
        query.max_items = 2;
        query.min_inference_budget_ms = 100;
        let scheduler = scheduler(engine.clone(), query);
        let stale = scheduler.submit(
            QueueKind::Query,
            input(),
            Instant::now() + Duration::from_millis(50),
            CancellationToken::new(),
        );
        let fresh = scheduler.submit(
            QueueKind::Query,
            input(),
            Instant::now() + Duration::from_secs(1),
            CancellationToken::new(),
        );
        let (stale, fresh) = tokio::join!(stale, fresh);
        assert!(matches!(stale, Err(AstraError::DeadlineExceeded(_))));
        assert!(fresh.is_ok());
        assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_timeout_is_not_retried() {
        let engine = Arc::new(TestEngine::with_outcomes(vec![Err(
            AstraError::DeadlineExceeded("engine timeout".into()),
        )]));
        let result = encode_batch_with_retry(
            engine.clone(),
            vec![input()],
            &InferenceRetryConfig::default().query,
            Instant::now() + Duration::from_secs(1),
            "query",
        )
        .await;
        assert!(result.is_err());
        assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_unavailable_retries_when_budget_allows() {
        let engine = Arc::new(TestEngine::with_outcomes(vec![
            Err(AstraError::Unavailable("transient".into())),
            Ok(()),
        ]));
        let result = encode_batch_with_retry(
            engine.clone(),
            vec![input()],
            &InferenceRetryConfig::default().query,
            Instant::now() + Duration::from_secs(2),
            "query",
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(engine.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_is_skipped_when_budget_is_insufficient() {
        let engine = Arc::new(TestEngine::with_outcomes(vec![Err(
            AstraError::Unavailable("transient".into()),
        )]));
        let result = encode_batch_with_retry(
            engine.clone(),
            vec![input()],
            &InferenceRetryConfig::default().query,
            Instant::now() + Duration::from_millis(100),
            "query",
        )
        .await;
        assert!(result.is_err());
        assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn document_retry_policy_remains_enabled() {
        let engine = Arc::new(TestEngine::with_outcomes(vec![
            Err(AstraError::Unavailable("transient".into())),
            Ok(()),
        ]));
        let result = encode_batch_with_retry(
            engine.clone(),
            vec![input()],
            &InferenceRetryConfig::default().document,
            Instant::now() + Duration::from_secs(3),
            "document",
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(engine.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn queue_depth_returns_to_zero() {
        let scheduler = scheduler(Arc::new(TestEngine::fixed(Duration::ZERO)), profile(2));
        for _ in 0..3 {
            assert!(scheduler
                .submit(
                    QueueKind::Query,
                    input(),
                    Instant::now() + Duration::from_secs(1),
                    CancellationToken::new(),
                )
                .await
                .is_ok());
        }
        assert!(scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn cancelled_job_is_removed_without_inference() {
        let engine = Arc::new(TestEngine::fixed(Duration::ZERO));
        let scheduler = scheduler(engine.clone(), profile(2));
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = scheduler
            .submit(
                QueueKind::Query,
                input(),
                Instant::now() + Duration::from_secs(1),
                cancel,
            )
            .await;
        assert!(matches!(result, Err(AstraError::Cancelled(_))));
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
    }
}
