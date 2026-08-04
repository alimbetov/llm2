# Local API Flow

FIX488 uses the current production gRPC path:

```text
AstraVectorIngestionFacade.IndexLogicalDocument
        |
        v
CreateMultiGranularityChunks
        |
        v
PostgreSQL chunks / embeddings / bindings / vector_outbox
        |
        v
Qdrant publisher
        |
        v
AstraVectorV004Control.ActivateDocumentVersion
        |
        v
AstraVectorV004Control.Search
```

Vector sync is checked through:

```text
AstraVectorIngestionFacade.GetDocumentVectorStatus
```

Canonical debug state is checked through:

```text
AstraVectorV004Control.DebugDocumentState
```

The tutorial uses `grpcurl` JSON names from `proto/astravector_embedding.proto`, for example `accessZoneId`, `callerAccessLevel`, `documentVersion`, `searchMode` and `includeDebug`.

