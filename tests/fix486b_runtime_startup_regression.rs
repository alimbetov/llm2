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

#[test]
fn access_zone_auto_creation_is_deterministic_for_a_stable_zone_code() {
    let registry =
        fs::read_to_string("src/access_zone_registry/mod.rs").expect("read access-zone registry");
    assert!(
        registry.contains("deterministic_access_zone_id(code)"),
        "auto-create must derive a stable UUID from the validated zone code"
    );
    assert!(
        !registry.contains("let new_id = Uuid::new_v4()"),
        "random zone UUIDs make clean-run hierarchy identities nondeterministic"
    );
}
