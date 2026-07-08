# TTL Delete Fencing

`document_versions.delete_operation_id` is the cleanup fencing token. A cleanup worker must set it while the document is still in `DELETING` and must finalize `DELETED` only with the same token.

Normal lifecycle transitions must not overwrite a document with an active `delete_operation_id`. Recovery may clear a stale token only when the stale-delete timeout policy fires.
