# Legal Hold and TTL Cleanup

Document TTL cleanup must not delete vector bindings or Qdrant points protected by `legal_hold=true`.

Expected delete points are selected with `legal_hold=false`. Reconciliation of extra Qdrant points must also be checked against PostgreSQL before deletion; legal-hold points discovered by Qdrant scroll are skipped and counted by `qdrant_cleanup_extra_points_skipped_legal_hold_total`.
