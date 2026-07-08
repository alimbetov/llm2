import importlib
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

import pytest

pytestmark = pytest.mark.load


@pytest.fixture(scope="session")
def astra_endpoint():
    endpoint = os.getenv("ASTRA_VECTOR_TEST_ENDPOINT")
    if not endpoint:
        if os.getenv("ASTRA_VECTOR_LOAD_SKIP_IF_NO_ENDPOINT", "true").lower() == "true":
            pytest.skip("ASTRA_VECTOR_TEST_ENDPOINT is not set")
        pytest.fail("ASTRA_VECTOR_TEST_ENDPOINT is required for load smoke")
    return endpoint


@pytest.fixture(scope="session")
def astra_modules():
    generated = os.path.join(os.getcwd(), "tests", "load", "generated")
    if generated not in sys.path:
        sys.path.insert(0, generated)

    try:
        return (
            importlib.import_module("astravector_embedding_pb2"),
            importlib.import_module("astravector_embedding_pb2_grpc"),
        )
    except Exception:
        if os.getenv("ASTRA_VECTOR_LOAD_GENERATE_STUBS", "true").lower() == "true":
            subprocess.check_call(["bash", "tests/load/generate_grpc_python_stubs.sh"])
            return (
                importlib.import_module("astravector_embedding_pb2"),
                importlib.import_module("astravector_embedding_pb2_grpc"),
            )
        raise


@pytest.fixture(scope="session")
def astra_stub(astra_endpoint, astra_modules):
    import grpc

    _, pb2_grpc = astra_modules
    channel = grpc.insecure_channel(astra_endpoint)
    return pb2_grpc.AstraVectorIngestionFacadeStub(channel)


def _ctx(pb2, i):
    return pb2.RequestContext(
        correlation_id=f"fix452-load-{i}",
        idempotency_key=f"fix452-load-{i}",
        caller_service="fix452-load-smoke",
        caller_user_id="load",
        caller_access_level=3,
    )


def _document(pb2, i):
    return pb2.DocumentIdentity(
        external_document_id=f"fix452-load-{i}",
        document_id=f"00000000-0000-0000-0000-{i:012d}",
        document_version=1,
        title=f"fix452 load document {i}",
        source_uri=f"load://fix452/{i}",
        source_type="LOAD_SMOKE",
        mime_type="text/plain",
        content_hash="",
    )


def _request(pb2, i):
    return pb2.IndexLogicalDocumentRequest(
        context=_ctx(pb2, i),
        access_zone_id="00000000-0000-0000-0000-000000000001",
        document=_document(pb2, i),
        blocks=[
            pb2.LogicalBlock(
                block_id=f"root-{i}",
                block_type=pb2.BlockType.DOCUMENT,
                text=f"Fix 4.5.2 load smoke document {i}. This document checks gRPC indexing.",
                order_index=0,
            )
        ],
    )


def test_10_concurrent_logical_documents_small(astra_stub, astra_modules):
    pb2, _ = astra_modules

    def index_one(i):
        response = astra_stub.IndexLogicalDocument(_request(pb2, i), timeout=30)
        assert response.operation is not None
        return response.operation.state

    with ThreadPoolExecutor(max_workers=10) as pool:
        states = list(pool.map(index_one, range(10)))

    assert len(states) == 10
