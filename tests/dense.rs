use astravector_runtime::dense::l2_normalize;
#[test]
fn dense_norm_is_one() {
    let v = l2_normalize(vec![3.0, 4.0], 2).unwrap();
    let norm = (v.iter().map(|x| x * x).sum::<f32>()).sqrt();
    assert!((norm - 1.0).abs() < 1e-6);
}
