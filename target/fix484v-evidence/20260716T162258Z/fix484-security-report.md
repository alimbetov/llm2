# fix484v Security Report

- Quality fixture structural/access contracts: 18 passed, 0 failed.
- Multi-zone retrieval uses compound `(access_zone_id, chunk_id)` identities: PASS.
- Graph seed expansion is zone-specific: PASS.
- Graph-related chunks preserve zone identity: PASS.
- Missing caller access level is rejected: PASS.
- Full network lifecycle testcontainers tests: 2 passed, 0 failed.
- Concurrent RetrieveContext smoke: 50 requests, PASS; runtime remained healthy.
- Panic, OOM, and deadlock observed by mandatory runs: 0.

These results establish regression protection for the touched query-processing path. They do not
replace a dedicated external penetration test, multi-node soak, or production traffic certification.
