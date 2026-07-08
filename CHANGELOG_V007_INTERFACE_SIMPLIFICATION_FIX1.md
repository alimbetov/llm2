# Changelog: AstraVector v007 interface simplification fix1

## Fixed

- Prevented unsafe `RetrieveContext` fallback to `INTERNAL` access level.
- Added strict `LogicalBlock[]` tree validation.
- Rejected unsupported `TTL_MODE_ABSOLUTE` instead of accepting it without effect.
- Rejected unsupported `AUTO_WHEN_READY` instead of accepting it without auto-activation.
- Changed delete facade operation state from final `DELETED` to async-correct `DELETE_SCHEDULED`.
- Improved `GetRuntimeHealth` from configured flags to live dependency checks.

## Added

- `OPERATION_STATE_DELETE_SCHEDULED` and `OPERATION_STATE_DELETING`.
- Source link safety validation.
- Migration `0020_v007_interface_simplification_fix1.sql` for `source_block_id`, `source_location`, `source_links`, and `logical_block_chunk_mapping` foundation.
- Documentation: `docs/V007_FIX1_INTERFACE_CONSISTENCY.md`.

## Known limitations

- `TTL_MODE_ABSOLUTE` remains unsupported in fix1 and is rejected.
- `AUTO_WHEN_READY` remains unsupported in fix1 and is rejected.
- Full block-to-chunk mapping write path is a follow-up item.
