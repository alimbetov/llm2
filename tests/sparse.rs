use astravector_runtime::sparse::build_sparse;
use std::collections::HashSet;
#[test]
fn sparse_merges_and_sorts() {
    let (i, v) = build_sparse(
        &[10, 2, 10],
        &[1, 1, 1],
        &[0.2, 0.3, 0.8],
        &HashSet::new(),
        0.01,
        256,
    )
    .unwrap();
    assert_eq!(i, vec![2, 10]);
    assert_eq!(v, vec![0.3, 0.8]);
}
