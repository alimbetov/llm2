use astravector_runtime::sparse::build_sparse;
use std::collections::HashSet;
#[test]
fn rejects_nan() {
    let e = build_sparse(&[10], &[1], &[f32::NAN], &HashSet::new(), 0.01, 256).unwrap_err();
    assert!(e.to_string().contains("NaN"));
}
