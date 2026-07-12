#![cfg(feature = "integration-tests")]

use astravector_runtime::persistence::Repository;
use sqlx::PgPool;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
async fn insert_parent(
    pool: &PgPool,
    zone: Uuid,
    document: Uuid,
    chunk: Uuid,
    content: &str,
    access_level: i16,
    document_status: &str,
    chunk_deleted: bool,
    expired: bool,
) {
    sqlx::query(
        r#"INSERT INTO astravector.document_versions(
             access_zone_id,document_id,document_version,content_hash,status,lifecycle_status,
             expires_at,activated_at)
           VALUES($1,$2,1,$3,$4,$4,CASE WHEN $5 THEN now()-interval '1 minute' END,now())"#,
    )
    .bind(zone)
    .bind(document)
    .bind("a".repeat(64))
    .bind(document_status)
    .bind(expired)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO astravector.content_chunks_v004(
             access_zone_id,id,root_chunk_id,source_chunk_id,parent_chunk_id,document_id,
             document_version,granularity,representation_type,sequence_no,target_token_count,
             actual_token_count,content,content_hash,tokenizer_version,chunking_profile_version,
             access_level,lifecycle_status,deleted_at,source_block_id)
           VALUES($1,$2,$2,$2,$2,$3,1,'PARENT','ORIGINAL',0,10,10,$4,$5,'test','fix480',
                  $6,'ACTIVE',CASE WHEN $7 THEN now() END,$8)"#,
    )
    .bind(zone)
    .bind(chunk)
    .bind(document)
    .bind(content)
    .bind("b".repeat(64))
    .bind(access_level)
    .bind(chunk_deleted)
    .bind(format!("block-{chunk}"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn lexical_exact_match_is_found_beyond_recent_parent_window() {
    let postgres = GenericImage::new("pgvector/pgvector", "pg16")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_DB", "astravector")
        .with_env_var("POSTGRES_USER", "astravector")
        .with_env_var("POSTGRES_PASSWORD", "astravector")
        .start()
        .await
        .unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://astravector:astravector@127.0.0.1:{port}/astravector"
    ))
    .await
    .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let repo = Repository { pool: pool.clone() };
    let zone = Uuid::new_v4();
    let other_zone = Uuid::new_v4();
    for (id, code) in [(zone, "1700"), (other_zone, "1800")] {
        sqlx::query("INSERT INTO astravector.access_zones(access_zone_id,access_zone_code,access_zone_name,status,default_ttl_days,ttl_policy_source,allow_never_expire) VALUES($1,$2,'fix480','ACTIVE',365,'CODE_MATRIX',false)")
            .bind(id).bind(code).execute(&pool).await.unwrap();
    }

    let target_document = Uuid::new_v4();
    let target_chunk = Uuid::new_v4();
    insert_parent(
        &pool,
        zone,
        target_document,
        target_chunk,
        "FIX480LEXICALTOKEN997 repairs the authoritative projection",
        1,
        "ACTIVE",
        false,
        false,
    )
    .await;
    sqlx::query("UPDATE astravector.content_chunks_v004 SET updated_at=now()-interval '30 days' WHERE access_zone_id=$1 AND id=$2")
        .bind(zone).bind(target_chunk).execute(&pool).await.unwrap();

    for index in 0..300 {
        insert_parent(
            &pool,
            zone,
            Uuid::new_v4(),
            Uuid::new_v4(),
            &format!("recent unrelated distractor parent number {index}"),
            1,
            "ACTIVE",
            false,
            false,
        )
        .await;
    }
    insert_parent(
        &pool,
        other_zone,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "FIX480LEXICALTOKEN997 foreign zone",
        1,
        "ACTIVE",
        false,
        false,
    )
    .await;
    insert_parent(
        &pool,
        zone,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "FIX480LEXICALTOKEN997 restricted",
        4,
        "ACTIVE",
        false,
        false,
    )
    .await;
    insert_parent(
        &pool,
        zone,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "FIX480LEXICALTOKEN997 deleted",
        1,
        "ACTIVE",
        true,
        false,
    )
    .await;
    insert_parent(
        &pool,
        zone,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "FIX480LEXICALTOKEN997 expired",
        1,
        "ACTIVE",
        false,
        true,
    )
    .await;

    let results = repo
        .search_active_parent_contexts_lexical_multi(
            &[zone],
            1,
            "FIX480LEXICALTOKEN997",
            None,
            5,
            1_000,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].parent.document_id, target_document);
    assert_eq!(results[0].parent.id, target_chunk);
    assert!(results[0].exact_match);
    assert!(results[0].lexical_score > 0.0);

    let second_document = Uuid::new_v4();
    let second_chunk = Uuid::new_v4();
    insert_parent(
        &pool,
        zone,
        second_document,
        second_chunk,
        "second hydrated parent",
        1,
        "ACTIVE",
        false,
        false,
    )
    .await;
    let stale_document = Uuid::new_v4();
    let stale_chunk = Uuid::new_v4();
    insert_parent(
        &pool,
        zone,
        stale_document,
        stale_chunk,
        "stale hydrated parent",
        1,
        "SUPERSEDED",
        false,
        false,
    )
    .await;
    let hydrated = repo
        .fetch_hydrated_search_contexts_multi(
            &[
                (zone, second_chunk, second_chunk),
                (zone, target_chunk, target_chunk),
                (zone, stale_chunk, stale_chunk),
            ],
            1,
            1_000,
        )
        .await
        .unwrap();
    assert_eq!(hydrated.len(), 2);
    assert_eq!(hydrated[0].document_id, second_document);
    assert_eq!(hydrated[0].matched_text, "second hydrated parent");
    assert_eq!(hydrated[0].parent_text, "second hydrated parent");
    assert_eq!(hydrated[1].document_id, target_document);
    assert!(!hydrated
        .iter()
        .any(|value| value.document_id == stale_document));
}
