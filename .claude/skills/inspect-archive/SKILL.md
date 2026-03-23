---
name: inspect-archive
description: Query and analyze the local session archive — sessions, encounters, detection decisions, SOAP notes
user-invocable: true
disable-model-invocation: true
arguments:
  - name: date
    description: "Date to inspect (YYYY-MM-DD) or 'today'"
    default: today
  - name: query
    description: "What to look for: summary, decisions, non-clinical, missing-soap, day-log, sensor-events"
    default: summary
---

# Inspect Local Archive

Query the local session archive for diagnostics, auditing, and analysis.

## Archive Structure

```
~/.transcriptionapp/archive/YYYY/MM/DD/
├── day_log.jsonl                          # Day-level events (config, splits, merges, SOAP)
├── <session_id>/
│   ├── metadata.json                      # Session metadata (mode, detection method, patient name, etc.)
│   ├── transcript.json                    # Full transcript
│   ├── notes.json                         # SOAP notes
│   ├── segments.jsonl                     # Per-segment timeline
│   ├── replay_bundle.json                 # Self-contained encounter replay test case
│   └── feedback.json                      # Physician feedback (if reviewed)
```

## Queries

### summary (default)
List all sessions for the date with key metrics.

```bash
ARCHIVE=~/.transcriptionapp/archive
DATE=$(date +%Y/%m/%d)  # or specific date

# List session directories
ls "$ARCHIVE/$DATE/" 2>/dev/null

# For each session, read metadata
for dir in "$ARCHIVE/$DATE"/*/; do
  if [ -f "$dir/metadata.json" ]; then
    python3 -c "
import json
with open('$dir/metadata.json') as f:
    m = json.load(f)
print(f\"  {m.get('sessionId','?')[:8]}  enc#{m.get('encounterNumber','?')}  {m.get('detectionMethod','?')}  words={m.get('wordCount','?')}  clinical={not m.get('likelyNonClinical', False)}  soap={'yes' if m.get('hasSoapNote') else 'no'}  patient={m.get('patientName','?')}\")" 2>/dev/null
  fi
done
```

### decisions
Show encounter detection decisions from replay bundles.

```bash
# For each session with a replay_bundle.json
for dir in "$ARCHIVE/$DATE"/*/; do
  if [ -f "$dir/replay_bundle.json" ]; then
    python3 -c "
import json
with open('$dir/replay_bundle.json') as f:
    b = json.load(f)
checks = b.get('detection_checks', [])
split = b.get('split_decision')
print(f\"  Session: $(basename $dir | cut -c1-8)  checks={len(checks)}  split={'yes: '+split['trigger'] if split else 'no'}\")" 2>/dev/null
  fi
done
```

### day-log
Parse the day-level orchestration log for the date.

```bash
DAY_LOG="$ARCHIVE/$DATE/day_log.jsonl"
if [ -f "$DAY_LOG" ]; then
  python3 -c "
import json
with open('$DAY_LOG') as f:
    for line in f:
        e = json.loads(line)
        print(f\"  {e.get('ts','?')[11:19]}  {e.get('event','?')}  {json.dumps({k:v for k,v in e.items() if k not in ('ts','event')}, separators=(',',':'))[:100]}\")"
fi
```

### non-clinical
Find sessions flagged as non-clinical (SOAP generation was skipped).

```bash
for dir in "$ARCHIVE/$DATE"/*/; do
  if [ -f "$dir/metadata.json" ]; then
    python3 -c "
import json
with open('$dir/metadata.json') as f:
    m = json.load(f)
if m.get('likelyNonClinical'):
    print(f\"  {m.get('sessionId','?')[:8]}  words={m.get('wordCount','?')}  method={m.get('detectionMethod','?')}\")" 2>/dev/null
  fi
done
```

### missing-soap
Find sessions that should have SOAP but don't.

```bash
for dir in "$ARCHIVE/$DATE"/*/; do
  if [ -f "$dir/metadata.json" ]; then
    python3 -c "
import json
with open('$dir/metadata.json') as f:
    m = json.load(f)
if not m.get('hasSoapNote') and not m.get('likelyNonClinical') and m.get('wordCount', 0) > 100:
    print(f\"  {m.get('sessionId','?')[:8]}  words={m.get('wordCount','?')}  method={m.get('detectionMethod','?')}\")" 2>/dev/null
  fi
done
```

### sensor-events
Extract sensor-related events from day log and replay bundles.

```bash
# Sensor transitions from day log
python3 -c "
import json
with open('$ARCHIVE/$DATE/day_log.jsonl') as f:
    for line in f:
        e = json.loads(line)
        if 'sensor' in e.get('event','').lower() or 'sensor' in json.dumps(e).lower():
            print(f\"  {e.get('ts','?')[11:19]}  {e.get('event','?')}\")" 2>/dev/null

# Sensor context from replay bundles
for dir in "$ARCHIVE/$DATE"/*/; do
  if [ -f "$dir/replay_bundle.json" ]; then
    python3 -c "
import json
with open('$dir/replay_bundle.json') as f:
    b = json.load(f)
transitions = b.get('sensor_transitions', [])
if transitions:
    print(f\"  Session $(basename $dir | cut -c1-8): {len(transitions)} sensor transitions\")" 2>/dev/null
  fi
done
```

## Report Format

After running queries, summarize:
1. Total sessions, encounter count, clinical vs non-clinical breakdown
2. Detection methods used (hybrid_llm, hybrid_sensor_confirmed, hybrid_sensor_timeout, flush, manual)
3. Any anomalies: missing SOAP, unusually short/long sessions, high merge-back count
4. For sensor queries: departure/arrival patterns and correlation with splits
