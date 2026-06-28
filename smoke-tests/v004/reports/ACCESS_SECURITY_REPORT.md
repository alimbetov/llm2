# AstraVector_v004 Access Security Report

## 1. Verdict
ACCESS_SECURITY_PASS

## 2. Environment
- Date: 2026-06-27T15:18:30Z
- ZONE_A: 11111111-1111-4111-8111-111111111111
- ZONE_B: 22222222-2222-4222-8222-222222222222
- Civil Code document_id: 72fd8953-9f11-5eef-a03c-ef47c3d40daa
- Zone B document_id: 00af7a8d-a963-5583-adbb-f7fc52273fa8
- Qdrant collection: astravector_smoke_v004
- gRPC endpoint: 127.0.0.1:55051

## 3. Summary
| Check | Status | Evidence |
|---|---|---|
| Search isolation | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/access-security-evidence.jsonl |
| Foreign ResolveParentContext | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/access-security |
| Foreign GetChunkGroup | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/access-security |
| Access level matrix | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/access-security |

## 4. Search Isolation
| Query | Request Zone | Expected | Actual | Status |
|---|---|---|---|---|
| Civil Code | ZONE_A | Civil Code only | results=10 | PASS |
| Civil Code | ZONE_B | no Civil Code | results=1 | PASS |
| Zone B secret | ZONE_A | no Zone B secret | results=6 | PASS |
| Zone B secret | ZONE_B | Zone B secret | results=1 | PASS |

## 5. Foreign ResolveParentContext
| Attack | Expected | Actual gRPC status | Leaked text | Leaked metadata | Status |
|---|---|---|---|---|---|
| ZONE_B resolves ZONE_A parent | NOT_FOUND/PERMISSION_DENIED | NOT_FOUND | 0 | 0 | PASS |

## 6. Foreign GetChunkGroup
| Attack | Expected | Actual gRPC status | Returned chunks | Leaked metadata | Status |
|---|---|---|---:|---|---|
| ZONE_B gets ZONE_A root | NOT_FOUND/PERMISSION_DENIED | NOT_FOUND | 0 | 0 | PASS |

## 7. Access Level Matrix
| Caller Level | Expected Visible Levels | Forbidden Secret Found | Status |
|---:|---|---|---|
| 1 | <= 1 | 0 | PASS |
| 2 | <= 2 | 0 | PASS |
| 3 | <= 3 | 0 | PASS |
| 4 | <= 4 | 0 | PASS |

## 8. Qdrant Evidence
| Metric | Value |
|---|---:|
| zone_a_qdrant_points | 1580 |
| zone_b_qdrant_points | 3 |

## 9. PostgreSQL Double Check Evidence
| Check | Value |
|---|---:|
| zone_a_parent_in_zone_a | 1 |
| zone_a_parent_in_zone_b | 0 |

## 10. Metrics
```json
{
  "verdict": "ACCESS_SECURITY_PASS",
  "zone_a_qdrant_points": 1580,
  "zone_b_qdrant_points": 3,
  "zone_a_search_results": 10,
  "zone_b_search_for_civil_code_results": 1,
  "zone_a_search_for_zone_b_secret_results": 6,
  "zone_b_search_for_zone_b_secret_results": 1,
  "foreign_parent_resolution_attempts": 1,
  "foreign_chunk_group_attempts": 1,
  "cross_zone_leakage_count": 0,
  "foreign_parent_text_returned": 0,
  "foreign_metadata_returned": 0,
  "access_level_violation_count": 0,
  "permission_denied_count": 0,
  "not_found_count": 2,
  "unexpected_ok_count": 0,
  "transport_error_count": 0
}
```

## 11. Remaining Risks
- timing side-channel not measured deeply
- auth/mTLS not covered unless implemented
- admin endpoint security not covered unless implemented
