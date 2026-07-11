# Quality Evidence Baselines

Runtime quality output is never tracked here. Official runs write to the stage-specific
`ASTRAVECTOR_QUALITY_OUTPUT_DIR` under the external evidence root. Local runs default to
`target/quality-reports`.

This directory may contain only reviewed, sanitized examples and schemas. Files must not
contain a live `quality_run_id`, machine identity, runtime PID, current Qdrant point count,
or current access-zone distribution.
