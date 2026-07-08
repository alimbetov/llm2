# Ingestion finalize recovery

During chunked `FinalizeLogicalDocumentIngestion`, AstraVector keeps `finalizing_heartbeat_at` fresh while the long `index_logical_document` call runs. This prevents cleanup from marking an active `FINALIZING` session as `FAILED` while indexing is still committing document/chunk/outbox rows.

Operators should alert on `ingestion_finalize_lost_ownership_total` and `ingestion_finalizing_stale_failed_total`.
