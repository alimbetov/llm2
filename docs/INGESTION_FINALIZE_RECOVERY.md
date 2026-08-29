# Ingestion finalize recovery

During chunked `FinalizeLogicalDocumentIngestion`, AstraVector keeps `finalizing_heartbeat_at` fresh while the long `index_logical_document` call runs. This prevents cleanup from marking an active `FINALIZING` session as `FAILED` while indexing is still committing document/chunk/outbox rows.

Session finalize uses server-owned `MANUAL` activation semantics. `AUTO_WHEN_READY` remains a defined but unsupported activation policy until a durable auto-activation lifecycle worker exists. A successful `FinalizeLogicalDocumentIngestion` means the session was accepted/finalized and indexing/vector publication work was created or completed; it does not mean the document is searchable.

Supported public consumer flow:

```text
StartLogicalDocumentIngestion
AppendLogicalDocumentBlocks
FinalizeLogicalDocumentIngestion
GetDocumentVectorStatus -> READY_TO_ACTIVATE
AstraVectorV004Control.ActivateDocumentVersion
Search or RetrieveContext
```

`session COMPLETED` and `document READY_TO_ACTIVATE` is a valid state. Consumers must explicitly activate the document version before expecting it to be searchable.

Operators should alert on `ingestion_finalize_lost_ownership_total` and `ingestion_finalizing_stale_failed_total`.
