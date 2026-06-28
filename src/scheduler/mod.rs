use crate::{
    config::BatchingConfig,
    error::AstraError,
    inference::{EmbeddingResult, InferenceEngine, InferenceInput},
};
use metrics::{gauge, histogram};
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

impl Scheduler {
    pub fn start(
        engine: Arc<dyn InferenceEngine>,
        cfg: BatchingConfig,
        max_consecutive_queries: usize,
    ) -> Self {
        let (query_tx, query_rx) = mpsc::channel(cfg.query.queue_capacity);
        let (document_tx, document_rx) = mpsc::channel(cfg.document.queue_capacity);
        let (health_tx, healthy) = watch::channel(true);
        tokio::spawn(async move {
            let ok = run_loop(engine, query_rx, document_rx, cfg, max_consecutive_queries)
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
}

async fn run_loop(
    engine: Arc<dyn InferenceEngine>,
    mut query_rx: mpsc::Receiver<Job>,
    mut document_rx: mpsc::Receiver<Job>,
    cfg: BatchingConfig,
    max_consecutive_queries: usize,
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

        let earliest_deadline = jobs.iter().map(|job| job.deadline).min().unwrap();
        if Instant::now() >= earliest_deadline {
            for job in jobs {
                let _ = job
                    .tx
                    .send(Err(AstraError::DeadlineExceeded("batch deadline".into())));
            }
            continue;
        }

        let inputs = jobs.iter().map(|job| job.input.clone()).collect();
        let inference_started = std::time::Instant::now();
        let result = tokio::time::timeout_at(earliest_deadline, engine.encode_batch(inputs)).await;
        histogram!("astravector_inference_duration_seconds")
            .record(inference_started.elapsed().as_secs_f64());

        match result {
            Ok(Ok(results)) if results.len() == jobs.len() => {
                for (job, result) in jobs.into_iter().zip(results) {
                    let _ = job.tx.send(Ok(result));
                }
            }
            Ok(Ok(_)) => {
                for job in jobs {
                    let _ = job.tx.send(Err(AstraError::Internal(
                        "engine result count mismatch".into(),
                    )));
                }
            }
            Ok(Err(error)) => {
                for job in jobs {
                    let _ = job.tx.send(Err(error.clone()));
                }
            }
            Err(_) => {
                for job in jobs {
                    let _ = job.tx.send(Err(AstraError::DeadlineExceeded(
                        "inference deadline".into(),
                    )));
                }
            }
        }
    }
    Ok(())
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
