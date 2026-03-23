---
name: monitor-continuous
description: Analyze continuous mode logs for debugging — day log events, segment timelines, detection decisions, sensor transitions
user-invocable: true
disable-model-invocation: true
arguments:
  - name: date
    description: "Date to analyze (YYYY-MM-DD) or 'today'"
    default: today
  - name: session
    description: "Specific session ID prefix (e.g. b2e0f61a) or 'all'"
    default: all
  - name: focus
    description: "Focus area: timeline, detections, sensors, soap, errors"
    default: timeline
---

# Monitor Continuous Mode

Analyze continuous mode structured logs for debugging encounter detection, SOAP generation, sensor behavior, and pipeline health.

## Log Tiers

1. **Day log** (`day_log.jsonl`) — orchestration events: config snapshot, encounter splits, merges, clinical checks, SOAP results
2. **Segments** (`segments.jsonl`) — per-segment timeline: timestamp, text, speaker, word counts
3. **Replay bundles** (`replay_bundle.json`) — complete encounter test cases: all LLM prompts/responses, sensor transitions, vision results, decisions

## Focus Areas

### timeline (default)

Show the day's event timeline from the day log.

```bash
ARCHIVE=~/.transcriptionapp/archive
DATE=$(date +%Y/%m/%d)  # adjust for specific date

python3 -c "
import json, sys
with open('$ARCHIVE/$DATE/day_log.jsonl') as f:
    for line in f:
        e = json.loads(line)
        ts = e.get('ts', '?')[11:19]
        event = e.get('event', '?')
        # Show key details per event type
        detail = ''
        if 'session_id' in e: detail += f\" sid={e['session_id'][:8]}\"
        if 'word_count' in e: detail += f\" words={e['word_count']}\"
        if 'trigger' in e: detail += f\" trigger={e['trigger']}\"
        if 'detection_method' in e: detail += f\" method={e['detection_method']}\"
        if 'patient_name' in e: detail += f\" patient={e['patient_name']}\"
        if 'error' in e: detail += f\" ERROR={e['error'][:60]}\"
        print(f'{ts}  {event:<30}{detail}')
"
```

### detections

Analyze encounter detection decisions from replay bundles.

```bash
for dir in "$ARCHIVE/$DATE"/*/; do
  [ -f "$dir/replay_bundle.json" ] || continue
  python3 -c "
import json
with open('$dir/replay_bundle.json') as f:
    b = json.load(f)
sid = '$(basename $dir)'[:8]
checks = b.get('detection_checks', [])
split = b.get('split_decision')
for i, c in enumerate(checks):
    sensor = 'departed' if c.get('sensor_context',{}).get('departed') else 'present'
    result = f\"complete={c.get('parsed_complete')} conf={c.get('parsed_confidence')}\" if c.get('success') else f\"FAILED: {c.get('error','?')[:40]}\"
    print(f\"  {sid} check {i+1}/{len(checks)}: words={c.get('word_count',0)} sensor={sensor} {result}\")
if split:
    print(f\"  {sid} SPLIT: trigger={split['trigger']} words={split['word_count']}\")
else:
    print(f\"  {sid} NO SPLIT (encounter continued or was flushed)\")
print()
" 2>/dev/null
done
```

### sensors

Extract sensor state transitions and their relationship to encounter splits.

```bash
for dir in "$ARCHIVE/$DATE"/*/; do
  [ -f "$dir/replay_bundle.json" ] || continue
  python3 -c "
import json
with open('$dir/replay_bundle.json') as f:
    b = json.load(f)
sid = '$(basename $dir)'[:8]
transitions = b.get('sensor_transitions', [])
if transitions:
    print(f'  {sid}: {len(transitions)} sensor transitions')
    for t in transitions:
        print(f\"    {t.get('ts','?')[11:19]}  {t.get('from_state','?')} -> {t.get('to_state','?')}\")
" 2>/dev/null
done
```

### soap

Check SOAP generation outcomes for each session.

```bash
for dir in "$ARCHIVE/$DATE"/*/; do
  [ -f "$dir/metadata.json" ] || continue
  python3 -c "
import json
with open('$dir/metadata.json') as f:
    m = json.load(f)
sid = m.get('sessionId','?')[:8]
has_soap = m.get('hasSoapNote', False)
non_clinical = m.get('likelyNonClinical', False)
words = m.get('wordCount', 0)
status = 'non-clinical (skipped)' if non_clinical else ('SOAP generated' if has_soap else 'MISSING SOAP')
print(f\"  {sid}  enc#{m.get('encounterNumber','?')}  words={words}  {status}\")
" 2>/dev/null
done
```

### errors

Find error events in day log and failed LLM calls in replay bundles.

```bash
# Day log errors
python3 -c "
import json
with open('$ARCHIVE/$DATE/day_log.jsonl') as f:
    for line in f:
        e = json.loads(line)
        if 'error' in e or 'fail' in e.get('event','').lower():
            print(f\"  {e.get('ts','?')[11:19]}  {e.get('event','?')}  {e.get('error','')[:80]}\")" 2>/dev/null

# Failed detection checks
for dir in "$ARCHIVE/$DATE"/*/; do
  [ -f "$dir/replay_bundle.json" ] || continue
  python3 -c "
import json
with open('$dir/replay_bundle.json') as f:
    b = json.load(f)
sid = '$(basename $dir)'[:8]
for c in b.get('detection_checks', []):
    if not c.get('success'):
        print(f\"  {sid} LLM FAIL: {c.get('error','?')[:80]}\")
" 2>/dev/null
done
```

## Report Format

After running, summarize:
1. Total events, encounters detected, SOAP notes generated
2. Detection decision breakdown (split types, merge-backs, skipped checks)
3. Sensor behavior (transition count, false departures, correlation with splits)
4. Errors or anomalies (failed LLM calls, missing SOAP, unusually short encounters)
5. Timing: average time between encounters, detection latency
