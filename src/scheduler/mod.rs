use crate::{
    config::{BatchingConfig, RetryPolicyConfig},
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
        inference_retry: RetryPolicyConfig,
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
        sender
            .try_send(Job {
                input,
                deadline,
                cancel: cancel.clone(),
                tx,
            })
            .map_err(|_| AstraError::ResourceExhausted("embedding queue full".into()))?;
        gauge!("astravector_queue_depth", "queue" => queue_label).increment(1.0);

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
    inference_retry: RetryPolicyConfig,
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
        let started = Instant::now();
        let mut jobs = Vec::new();
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

            if job.cancel.is_cancelled() || Instant::now() >= job.deadline {
                gauge!("astravector_queue_depth", "queue" => queue_label).decrement(1.0);
                let _ = job.tx.send(Err(AstraError::DeadlineExceeded(
                    "expired before inference".into(),
                )));
                continue;
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
        }

        if jobs.is_empty() {
            continue;
        }

        gauge!("astravector_queue_depth", "queue" => queue_label).decrement(jobs.len() as f64);
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
        let result =
            encode_batch_with_retry(engine.clone(), inputs, &inference_retry, earliest_deadline)
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
                    counter!("inference_retry_success_after_retry_total", "operation" => "encode_batch").increment(1);
                }
                return Ok(outputs);
            }
            Ok(Err(err))
                if is_retryable_inference_error(&err, policy) && attempt < max_attempts =>
            {
                counter!("inference_retry_attempts_total", "operation" => "encode_batch")
                    .increment(1);
                let delay = retry_delay(policy, attempt);
                histogram!("inference_retry_delay_ms", "operation" => "encode_batch")
                    .record(delay.as_millis() as f64);
                tracing::warn!(attempt, error=%err, "inference encode_batch transient failure; retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Ok(Err(err)) if is_retryable_inference_error(&err, policy) => {
                counter!("inference_retry_exhausted_total", "operation" => "encode_batch")
                    .increment(1);
                return Err(err);
            }
            Ok(Err(err)) => return Err(err),
            Err(_) if policy.retry_on_timeout && attempt < max_attempts => {
                counter!("inference_retry_attempts_total", "operation" => "encode_batch")
                    .increment(1);
                let delay = retry_delay(policy, attempt);
                histogram!("inference_retry_delay_ms", "operation" => "encode_batch")
                    .record(delay.as_millis() as f64);
                tracing::warn!(attempt, "inference encode_batch timeout; retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(_) => {
                counter!("inference_retry_exhausted_total", "operation" => "encode_batch")
                    .increment(1);
                return Err(AstraError::DeadlineExceeded("inference deadline".into()));
            }
        }
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
