use std::fs;
use std::path::Path;

#[test]
fn profile_yaml_files_exist() {
    for path in [
        "config/application.yaml",
        "config/application-dev.yaml",
        "config/application-test.yaml",
        "config/application-prod.yaml",
    ] {
        assert!(Path::new(path).exists(), "missing {path}");
    }
}

#[test]
fn config_loader_mentions_profile_overlay_rules() {
    let src = fs::read_to_string("src/config/mod.rs").expect("read config module");
    assert!(src.contains("ASTRAVECTOR_PROFILE"));
    assert!(src.contains("ASTRAVECTOR_ENV"));
    assert!(src.contains("application-{profile}.yaml"));
    assert!(src.contains("merge_yaml"));
    assert!(src.contains("ASTRAVECTOR_PROFILE_CONFIG"));
}

#[test]
fn production_profile_does_not_default_to_local_database() {
    let prod = fs::read_to_string("config/application-prod.yaml").expect("read prod profile");
    assert!(prod.contains("service:"));
    assert!(prod.contains("environment: production"));
    assert!(prod.contains("url: ${ASTRAVECTOR_DB_URL:-}"));
    assert!(!prod.contains("localhost:5432/astravector}"));
}

#[test]
fn config_profile_docs_exist() {
    let docs = fs::read_to_string("docs/CONFIG_PROFILES.md").expect("read config profile docs");
    assert!(docs.contains("config/application-dev.yaml"));
    assert!(docs.contains("ASTRAVECTOR_PROFILE"));
    assert!(docs.contains("ASTRAVECTOR_ENV"));
}
