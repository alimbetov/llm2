use astravector_runtime::{
    chunking::{
        ChunkingEngine, ChunkingProfile, ConservativeTokenCounter, Granularity, SizeProfile,
        SourceChunkStorageMode,
    },
    domain::SearchRepresentation,
    relevance,
};
use uuid::Uuid;
#[test]
fn enum_mapping_is_explicit() {
    assert_eq!(
        SearchRepresentation::SyntheticQuestion.as_db_str(),
        "SYNTHETIC_QUESTION"
    );
    assert_eq!(SearchRepresentation::KeyFact.as_db_str(), "KEY_FACT");
}
#[test]
fn multi_granularity_is_deterministic() {
    let e = ChunkingEngine::new(ConservativeTokenCounter);
    let p = ChunkingProfile {
        version: "v1".into(),
        parent: SizeProfile {
            target: 10,
            min: 4,
            max: 20,
            overlap: 2,
        },
        sub180: SizeProfile {
            target: 6,
            min: 3,
            max: 10,
            overlap: 1,
        },
        sub260: SizeProfile {
            target: 8,
            min: 4,
            max: 12,
            overlap: 1,
        },
    };
    let z = Uuid::new_v4();
    let d = Uuid::new_v4();
    let a = e
        .chunk(
            z,
            d,
            1,
            "One sentence. Two sentence. Three sentence. Four sentence.",
            &p,
            SourceChunkStorageMode::FullText,
        )
        .unwrap();
    let b = e
        .chunk(
            z,
            d,
            1,
            "One sentence. Two sentence. Three sentence. Four sentence.",
            &p,
            SourceChunkStorageMode::FullText,
        )
        .unwrap();
    assert_eq!(
        a.iter().map(|x| x.id).collect::<Vec<_>>(),
        b.iter().map(|x| x.id).collect::<Vec<_>>()
    );
    assert!(a.iter().any(|x| x.granularity == Granularity::Sub180));
    assert!(a.iter().any(|x| x.granularity == Granularity::Sub260));
}
#[test]
fn dense_is_not_lexical() {
    let d = relevance::cosine(&[1.0, 0.0], &[0.0, 1.0]);
    assert_eq!(d, 0.0);
    let r = relevance::combine(d, 0.0, 1.0, None);
    assert!(r.final_score < 0.5);
}
