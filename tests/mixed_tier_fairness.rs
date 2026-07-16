use astravector_runtime::config::QueryProcessingConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[tokio::test]
async fn extended_work_cannot_consume_the_reserved_single_capacity() {
    let config = QueryProcessingConfig::default();
    let capacity = config.extended.admission_weight + config.single_admission_weight;
    let semaphore = Arc::new(Semaphore::new(capacity as usize));
    let extended = semaphore
        .clone()
        .acquire_many_owned(config.extended.admission_weight)
        .await
        .expect("extended admission");

    let single = tokio::time::timeout(
        Duration::from_millis(50),
        semaphore
            .clone()
            .acquire_many_owned(config.single_admission_weight),
    )
    .await
    .expect("single tier is not starved")
    .expect("single admission");
    assert_eq!(semaphore.available_permits(), 0);

    drop(single);
    drop(extended);
    assert_eq!(semaphore.available_permits(), capacity as usize);
}

#[test]
fn configured_tier_weights_are_monotonic_and_bounded() {
    let config = QueryProcessingConfig::default();
    assert!(config.single_admission_weight > 0);
    assert!(config.standard.admission_weight >= config.single_admission_weight);
    assert!(config.extended.admission_weight >= config.standard.admission_weight);
    assert!(config.extended.admission_weight <= 64);
}
