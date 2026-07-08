#![cfg(feature = "integration-tests")]

use astravector_runtime::pb::{
    astra_vector_retrieval_facade_client::AstraVectorRetrievalFacadeClient, AccessLevel,
    RequestContext, ResponseDetail, RetrievalProfile, RetrieveContextRequest,
};
use futures::future::join_all;
use std::{
    env,
    time::{Duration, Instant},
};
use tonic::{transport::Channel, Request};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "smoke-load is opt-in; set ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT=http://host:port"]
async fn test_smoke_load_retrieve_context_100_concurrent_graph_rag() {
    let endpoint = match env::var("ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT")
        .or_else(|_| env::var("ASTRA_VECTOR_TEST_ENDPOINT"))
    {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT is not set; skipping external smoke load"
            );
            return;
        }
    };
    let access_zone_id = env::var("ASTRA_VECTOR_SMOKE_ACCESS_ZONE_ID")
        .unwrap_or_else(|_| "00000000-0000-0000-0000-000000000001".to_string());
    let query = env::var("ASTRA_VECTOR_SMOKE_QUERY")
        .unwrap_or_else(|_| "production candidate retrieve context smoke".to_string());
    let concurrency: usize = env::var("ASTRA_VECTOR_SMOKE_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .clamp(1, 100);
    let timeout = Duration::from_secs(
        env::var("ASTRA_VECTOR_SMOKE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5),
    );

    let channel = Channel::from_shared(endpoint)
        .expect("valid smoke endpoint URI")
        .connect_timeout(timeout)
        .timeout(timeout)
        .connect()
        .await
        .expect("connect smoke RetrieveContext endpoint");
    let client = AstraVectorRetrievalFacadeClient::new(channel);

    let started = Instant::now();
    let tasks = (0..concurrency).map(|i| {
        let mut client = client.clone();
        let access_zone_id = access_zone_id.clone();
        let query = query.clone();
        async move {
            let mut request = Request::new(RetrieveContextRequest {
                context: Some(RequestContext {
                    correlation_id: format!("fix462-smoke-load-{i}"),
                    idempotency_key: String::new(),
                    caller_service: "fix462-smoke-load".into(),
                    caller_user_id: "smoke-load".into(),
                    caller_access_level: AccessLevel::Restricted as i32,
                }),
                access_zone_id: access_zone_id.clone(),
                question: query,
                access_zone_ids: vec![access_zone_id],
                access_zone_code: String::new(),
                access_zone_codes: Vec::new(),
                profile: RetrievalProfile::Balanced as i32,
                max_contexts: 5,
                filters: Vec::new(),
                response_detail: ResponseDetail::Standard as i32,
                enable_graph_expansion: true,
                graph_max_hops: 1,
                graph_max_related_contexts: 5,
            });
            request.set_timeout(timeout);
            client
                .retrieve_context(request)
                .await
                .map(|r| r.into_inner().contexts.len())
        }
    });

    let results = join_all(tasks).await;
    let success = results.iter().filter(|r| r.is_ok()).count();
    let success_rate = success as f64 / concurrency as f64;
    assert!(
        success_rate >= 0.99,
        "success_rate={success_rate:.3}, concurrency={concurrency}, results={results:?}"
    );
    assert!(
        started.elapsed() < timeout * 2,
        "smoke load exceeded aggregate timeout budget"
    );
}
