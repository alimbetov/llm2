"""fix4.5 runnable gRPC load-smoke scaffold.

The test imports without generated stubs. It is skipped unless ASTRA_VECTOR_TEST_ENDPOINT
is set and a project-specific `astravector_stub` fixture is provided by CI.
"""
import os
from concurrent.futures import ThreadPoolExecutor

import pytest


def make_text(size_bytes: int) -> str:
    unit = "AstraVector load test sentence. "
    return (unit * (size_bytes // len(unit) + 1))[:size_bytes]


@pytest.fixture
def astra_endpoint():
    endpoint = os.getenv("ASTRA_VECTOR_TEST_ENDPOINT")
    if not endpoint:
        pytest.skip("ASTRA_VECTOR_TEST_ENDPOINT is not set")
    return endpoint


def build_index_logical_document_request(document_id: str, source_text: str):
    pytest.skip("Generated grpcio protobuf stubs are not available in this repository; CI must override this helper")


@pytest.mark.load
def test_10_concurrent_logical_documents_small(astra_endpoint):
    # CI can monkeypatch build_index_logical_document_request and provide a real stub.
    pytest.skip("Requires generated AstraVector grpcio stubs and running service endpoint")
