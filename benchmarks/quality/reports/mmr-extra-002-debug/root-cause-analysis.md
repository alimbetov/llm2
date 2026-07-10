# mmr-extra-002 root cause analysis

## Observation

The previous live run returned the reconciliation siblings `tech-recon-run-001` through
`tech-recon-run-004` but omitted `tech-recon-run-005`, the original PostgreSQL parent
containing the `metrics` aspect. The quality fixture therefore failed with
`MMR_ASPECT_COVERAGE_LOW`.

## Root cause

The weak-evidence gate ran before MMR. Its ordinary partial-evidence rule required two
matched lexical terms, so a valid sibling that represented one aspect of a multi-aspect
question could be removed before the MMR pool was built. Increasing a final result limit
cannot recover a candidate that is absent from that pool.

## Fix

`partial_multi_aspect_candidate_passes` is a separate pre-MMR admission rule. It admits a
partial candidate only when the query has multiple meaningful aspects, the candidate belongs
to a document that already has a strong positive seed, it passes the configured score gate,
and it is neither a root container nor negative evidence. The rule is independent of query,
block, document, and fixture identifiers. Broad-coverage reinforcement no longer treats the
word `and` as a sufficient signal.

MMR and merge score ties are now resolved deterministically by stable result identity; MMR
ties additionally prefer higher relevance, then lower maximum similarity, then identity.

## Evidence

The full model-backed MMR smoke run `fix477-mmr-20260711-002718` passed 10/10. Its final
candidate trace includes `tech-recon-run-005` at rank 7 with `mmr_score=0.38895378`; the
required `metrics` aspect is present. Unit regressions cover multi-aspect admission,
negative-evidence rejection, and 50 order rotations for MMR tie determinism.
