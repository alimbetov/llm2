#![allow(dead_code)]
#![allow(clippy::result_large_err)]
#![allow(clippy::too_many_arguments)]

pub mod access_zone_registry;
pub mod adaptive;
pub mod cache;
pub mod checksum;
pub mod config;
pub mod contract;
pub mod dense;
pub mod error;
pub mod graph;
pub mod grpc;
pub mod health;
pub mod inference;
pub mod ingestion_cleanup;
pub mod metrics;
pub mod persistence;
pub mod provider;
pub mod recovery;
pub mod retention;
pub mod scheduler;
pub mod security;
pub mod sparse;
pub mod tokenizer;

pub mod pb {
    tonic::include_proto!("astravector.embedding.v1");
}

pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("astravector_descriptor");

pub mod bindings;
pub mod chunking;
pub mod domain;
pub mod enrichment;
pub mod lifecycle;
pub mod outbox;
pub mod qdrant;
pub mod reconciliation;
pub mod relevance;
pub mod reliability;
pub mod retrieval;
pub mod shutdown;
pub mod smoke_failpoints;
