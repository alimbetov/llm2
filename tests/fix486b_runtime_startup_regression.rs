use std::fs;

#[test]
fn runtime_creates_required_qdrant_collection_before_readiness_evaluation() {
    let main = fs::read_to_string("src/main.rs").expect("read runtime main");
    let ensure = main
        .find("qdrant.ensure_collection(cfg.dense.dimension)")
        .expect("auto-create must ensure the required Qdrant collection during startup");
    let readiness = main
        .find("let mut initial_ready = scheduler.healthy()")
        .expect("find initial readiness evaluation");
    assert!(
        ensure < readiness,
        "Qdrant collection creation must precede initial readiness evaluation"
    );
}
