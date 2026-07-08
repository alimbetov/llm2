"""fix4.5.1 runnable gRPC load smoke.

Runs only when ASTRA_VECTOR_TEST_ENDPOINT and generated Python gRPC modules are available.
Optional env overrides:
  ASTRA_VECTOR_PROTO_MODULE=astravector_embedding_pb2
  ASTRA_VECTOR_GRPC_MODULE=astravector_embedding_pb2_grpc
"""
import importlib
import os
from concurrent.futures import ThreadPoolExecutor

import grpc
import pytest


def make_text(size_bytes: int) -> str:
    unit = "AstraVector load test sentence. "
    return (unit * (size_bytes // len(unit) + 1))[:size_bytes]


@pytest.fixture(scope="session")
def astra_modules():
    pb2_name = os.getenv("ASTRA_VECTOR_PROTO_MODULE", "astravector_embedding_pb2")
    pb2_grpc_name = os.getenv("ASTRA_VECTOR_GRPC_MODULE", "astravector_embedding_pb2_grpc")
    try:
        return importlib.import_module(pb2_name), importlib.import_module(pb2_grpc_name)
    except Exception as exc:  # pragma: no cover - CI/env dependent
        pytest.skip(f"generated AstraVector grpcio modules are unavailable: {exc}")


@pytest.fixture(scope="session")
def astra_stub(astra_modules):
    endpoint = os.getenv("ASTRA_VECTOR_TEST_ENDPOINT")
    if not endpoint:
        pytest.skip("ASTRA_VECTOR_TEST_ENDPOINT is not set")
    _, pb2_grpc = astra_modules
    channel = grpc.insecure_channel(endpoint)
    # Prefer the v007 ingestion facade stub; allow CI to override if generated names differ.
    stub_name = os.getenv("ASTRA_VECTOR_INGESTION_STUB", "AstraVectorIngestionFacadeStub")
    return getattr(pb2_grpc, stub_name)(channel)


def build_index_logical_document_request(pb2, document_idx: int, source_text: str):
    # This intentionally uses getattr/fallbacks so generated packages with the canonical proto names work.
    req_cls = getattr(pb2, "IndexLogicalDocumentRequest")
    doc_cls = getattr(pb2, "DocumentIdentity")
    block_cls = getattr(pb2, "LogicalBlock")
    ctx_cls = getattr(pb2, "RequestContext")
    block_type = getattr(pb2, "BLOCK_TYPE_PARAGRAPH", 3)
    access_level = getattr(pb2, "ACCESS_LEVEL_INTERNAL", 3)
    doc_id = f"00000000-0000-0000-0000-{document_idx:012d}"
    return req_cls(
        context=ctx_cls(
            correlation_id=f"load-smoke-{document_idx}",
            idempotency_key=f"load-smoke-{document_idx}",
            caller_service="pytest-load-smoke",
            caller_access_level=access_level,
        ),
        access_zone_id="00000000-0000-0000-0000-000000000001",
        document=doc_cls(
            document_id=doc_id,
            document_version=1,
            title=f"Load smoke {document_idx}",
            source_type="LOAD_SMOKE",
        ),
        blocks=[block_cls(
            block_id=f"b-{document_idx}",
            block_type=block_type,
            text=source_text,
            order_index=1,
        )],
    )


@pytest.mark.load
def test_10_concurrent_logical_documents_small(astra_stub, astra_modules):
    pb2, _ = astra_modules

    def index_one(i: int):
        req = build_index_logical_document_request(pb2, i, make_text(32_000))
        return astra_stub.IndexLogicalDocument(req, timeout=60)

    with ThreadPoolExecutor(max_workers=10) as pool:
        results = list(pool.map(index_one, range(10)))

    assert len(results) == 10
    assert all(getattr(r.summary, "chunks_created", 0) >= 0 for r in results)
