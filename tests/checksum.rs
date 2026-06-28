use astravector_runtime::checksum;
#[tokio::test]
async fn validates_sha256() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"abc").unwrap();
    checksum::verify(
        f.path().to_str().unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        true,
    )
    .await
    .unwrap();
}
