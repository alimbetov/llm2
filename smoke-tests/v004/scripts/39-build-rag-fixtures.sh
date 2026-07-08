#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env

python3 - "$SMOKE_ROOT/fixtures/rag-questions-civil-code.json" "$ASTRAVECTOR_CORPUS_DIR" "$SMOKE_ROOT/fixtures/rag-questions-civil-code.resolved.json" <<'PY'
import json, pathlib, sys
fixture = pathlib.Path(sys.argv[1])
corpus = pathlib.Path(sys.argv[2])
out = pathlib.Path(sys.argv[3])
text = corpus.read_text(encoding="utf-8")
items = json.loads(fixture.read_text(encoding="utf-8"))
resolved = []
for item in items:
    phrase = item.get("expected_phrase")
    copy = dict(item)
    if not phrase:
        copy["fixture_status"] = "HARD_NEGATIVE"
        copy["expected_source"] = None
        resolved.append(copy)
        continue
    start = text.find(phrase)
    if start < 0:
        copy["fixture_status"] = "INVALID_FIXTURE"
        copy["expected_source"] = None
        resolved.append(copy)
        continue
    end = start + len(phrase)
    copy["fixture_status"] = "VALID"
    copy["expected_source"] = {
        "file": str(corpus),
        "byte_start": len(text[:start].encode("utf-8")),
        "byte_end": len(text[:end].encode("utf-8")),
        "snippet": text[max(0, start-500):min(len(text), end+500)],
        "article": None,
        "section": None
    }
    resolved.append(copy)
out.write_text(json.dumps(resolved, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"resolved={out} total={len(resolved)} valid={sum(1 for x in resolved if x.get('fixture_status')=='VALID')} invalid={sum(1 for x in resolved if x.get('fixture_status')=='INVALID_FIXTURE')}")
PY
