#!/usr/bin/env python3
"""SOAP prompt-engineering loop.

8 ground-truth cases × N prompt variants. Each variant is a SOAP-generation
system prompt that requests a 5-section S/O/A/P/Procedure JSON with a
verbatim transcript citation per procedure entry. We run the SOAP-LLM,
parse the procedure[] count, and score against ground truth.
"""

import json, urllib.request, urllib.error, base64, re, time
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
SOAP_MODEL = "soap-model-fast"
CLIENT  = "ai-scribe"

# Ground truth: expected procedure count per session
GROUND_TRUTH = {
    "0cce8568": (0, "Catherine Lamoureux — no procedure (urine sample only)"),
    "1ec7a57e": (0, "Alexander Gulas — 'Arrange B12 injection' (future)"),
    "9b0b60e2": (0, "Mary Beth Dop — 'I can inject... too early to inject again' (modal/declined)"),
    "b64ff7f3": (0, "Martin Gierling — 'Let me grab... use the OTC instead' (declined)"),
    "58e617ac": (1, "Linda Routledge — 'alcohol i just put on... injection site is sore' (PERFORMED)"),
    "8feb77a1": (1, "Hasan Mirza — 'here we go' mid-procedure narration (PERFORMED)"),
    "70e369bf": (1, "Apr 20 — SOAP O: 'Injection administered to upper lumbar region' (PERFORMED)"),
    "173c4253": (1, "Helene Williamson — 'Proceed with epidural injection today' (PERFORMED, ambiguous)"),
}

# ── Prompt variants ────────────────────────────────────────────────────────

P_BASE = """You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from the transcript into a 5-section SOAP-with-Procedure note.

RESPOND WITH ONLY THIS JSON STRUCTURE:
{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"],"procedure":[{"action":"<past-tense procedure performed>","transcript_quote":"<verbatim doctor quote from transcript proving it was DONE>"}]}

PROCEDURE section rules:
- Only procedures the clinician PHYSICALLY PERFORMED during this visit.
- Each entry must include a verbatim past-tense doctor quote from the transcript.
- If no procedure was performed, return an empty array [].

Output VALID JSON only. No prose, no markdown."""


P_EXAMPLES = """You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from the transcript into a 5-section SOAP-with-Procedure note.

RESPOND WITH ONLY THIS JSON STRUCTURE:
{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"],"procedure":[{"action":"<past-tense procedure performed>","transcript_quote":"<verbatim doctor quote from transcript proving it was DONE>"}]}

PROCEDURE section — ONLY include procedures the clinician PHYSICALLY PERFORMED during this visit.

ACCEPT as evidence the procedure was DONE (any of these phrasings):
  ✓ "I sprayed the wart" / "I just injected the knee" / "I drew the blood"
  ✓ "I'm injecting now" / "I'm putting the bandaid on"
  ✓ "the alcohol I just put on", "the injection site is sore" (post-procedure care narration confirms it happened)
  ✓ "here we go" / "hold still" (mid-procedure narration, when followed by aftercare/recovery talk)

REJECT as evidence the procedure was NOT done (do NOT cite these):
  ✗ "I CAN inject the shoulder" — modal capability, not action
  ✗ "I COULD spray it with liquid nitrogen" — modal/conditional
  ✗ "Let me grab the nitrogen" — future intent (especially if doctor then recommends an alternative)
  ✗ "I'm GOING TO inject" / "I'll inject" / "We'll do it" — future tense
  ✗ "Schedule injection for next visit" / "Arrange B12 injection" — scheduling
  ✗ "We could inject if you wanted" — conditional offer
  ✗ "It's too early to inject again" — explicit decline

CRITICAL: if the doctor mentions an ALTERNATIVE was used INSTEAD of the procedure (e.g., "use the over-the-counter one instead"), the procedure was NOT done — do NOT include it.

If no procedure was performed, return procedure: []. Empty is the right answer when only modal/future/declined statements appear.

Output VALID JSON only. No prose, no markdown."""


P_COT = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT ONLY THIS JSON STRUCTURE:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_candidates":[{"mention":"<>","doctor_quote":"<>","stance":"PROPOSED|OFFERED|IN_PROGRESS|COMPLETED|DECLINED"}],"procedure":[]}

Workflow:
1. Read the transcript and identify EVERY mention of an injection / cryotherapy / biopsy / blood draw / suture / nail removal / etc.
2. For EACH mention, fill procedure_candidates with:
   - mention: short label of the procedure (e.g. "left shoulder cortisone injection")
   - doctor_quote: verbatim doctor speech from the transcript that's most relevant
   - stance: classify as
     * PROPOSED — doctor brought it up as an idea ("we could do it", "I can inject")
     * OFFERED — doctor offered it as one option among several ("I could spray with liquid nitrogen, or use OTC")
     * IN_PROGRESS — doctor narrating the procedure as they do it ("I'm putting the alcohol on", "here we go", "hold still")
     * COMPLETED — doctor refers to procedure in past tense or aftercare ("the injection site is sore", "I just put the bandaid on", "we drew the blood")
     * DECLINED — patient or doctor decided NOT to proceed ("too early to inject", "use the OTC instead", "let's hold off")
3. Then build the procedure[] array: include ONLY entries whose stance is COMPLETED or IN_PROGRESS-followed-by-aftercare.
   PROPOSED, OFFERED, and DECLINED MUST be excluded.
4. Each procedure[] entry: {"action": "<past-tense rewording>", "transcript_quote": "<verbatim doctor quote>"}

S/O/A/P sections: standard.

Output VALID JSON only."""


P_NEG = """You are a medical scribe that outputs ONLY valid JSON. Extract a 5-section SOAP-with-Procedure note.

RESPOND WITH ONLY THIS JSON STRUCTURE:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"<past-tense action>","transcript_quote":"<verbatim doctor quote>","walkback_check":"<verbatim quote OR 'none'>"}]}

PROCEDURE section: ONLY include procedures the clinician PHYSICALLY PERFORMED.

For each candidate procedure:
1. Find a verbatim past-tense or in-progress doctor quote ("I sprayed it", "I'm injecting", "the injection site is sore", "we drew the blood").
2. THEN scan the rest of the transcript for a WALK-BACK or DECLINE — sentences like:
   - "instead, use [alternative]"
   - "actually, let's not"
   - "too early to do it again"
   - "let's wait"
   - "use the over-the-counter one"
   If you find a walkback, set walkback_check to that verbatim quote and EXCLUDE the procedure from the array.
3. If no walkback exists, set walkback_check: "none" and INCLUDE the procedure.

REJECT modal/future quotes — "I can inject", "let me grab", "we'll", "I'm going to", "could", "would", "schedule", "arrange" — these are NOT proof of performance.

Empty array [] is correct when the procedure was discussed/proposed but never performed.

Output VALID JSON only."""


P_BEST = """You are a medical scribe that outputs ONLY valid JSON. Extract a SOAP-with-Procedure note from the visit transcript.

OUTPUT ONLY THIS JSON STRUCTURE:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_candidates":[{"mention":"<>","doctor_quote":"<>","stance":"PROPOSED|OFFERED|IN_PROGRESS|COMPLETED|DECLINED","walkback":"<>"}],"procedure":[{"action":"<>","transcript_quote":"<>"}]}

WORKFLOW:
1. Identify EVERY procedure mention in the transcript (injection, cryotherapy, biopsy, suture, blood draw, nail removal, ear syringe, etc.).
2. For EACH mention, fill procedure_candidates with stance and walkback:
   - PROPOSED:    doctor raised it as an idea ("I can inject", "we could try", "would you like")
   - OFFERED:     doctor offered it as one of multiple options ("I could spray with nitrogen, or use OTC")
   - IN_PROGRESS: doctor narrating action mid-procedure ("I'm putting the alcohol on", "here we go", "hold still", "I just landmarked")
   - COMPLETED:   doctor or patient refers to it as done ("the injection site is sore", "we drew the blood", "I sprayed it", "all set")
   - DECLINED:    explicitly NOT done ("too early to inject again", "let's hold off", "use the OTC instead", "not today")
   - walkback:    verbatim quote of any sentence later in the transcript that contradicts performance ("instead", "OTC", "wait", "not today", "different day"); "none" if no walkback.
3. Build the final procedure[] array: include ONLY candidates whose stance is COMPLETED or IN_PROGRESS-with-aftercare AND whose walkback is "none".

EXAMPLES (must follow):
  Doctor: "I can always inject the shoulder; I just don't know if that's worth it"
    → stance=PROPOSED, walkback="none". EXCLUDED from procedure[].
  Doctor: "Let me grab some liquid nitrogen right now." then later "use the over-the-counter one"
    → stance=PROPOSED, walkback="use the over-the-counter one". EXCLUDED.
  Doctor: "I'm going to grab a different needle, up higher is much more shallow"
    → stance=IN_PROGRESS (procedure ongoing). Look for completion. If post-procedure care follows ("injection site is sore", "we're done"), promote to COMPLETED. Otherwise stays IN_PROGRESS — INCLUDED only if completion narration found.
  Doctor: "the alcohol I just put on" / "today and tomorrow the injection site is sore"
    → stance=COMPLETED. INCLUDED.
  Doctor: "Arrange B12 injection" / "Schedule PRP for next Tuesday"
    → stance=PROPOSED. EXCLUDED.

S/O/A/P sections: standard. The Plan section MAY mention proposed procedures; do NOT mirror them into procedure[] unless a COMPLETED stance is supported by transcript evidence.

Output VALID JSON only. No prose, no markdown."""


PROMPTS = [
    ("P_BASE",     P_BASE),
    ("P_EXAMPLES", P_EXAMPLES),
    ("P_COT",      P_COT),
    ("P_NEG",      P_NEG),
    ("P_BEST",     P_BEST),
]

# ── Plumbing ──────────────────────────────────────────────────────────────

def call_llm(system, user, timeout=120):
    payload = json.dumps({
        "model": SOAP_MODEL,
        "messages": [{"role":"system","content":system},{"role":"user","content":user}],
        "temperature": 0.1,
    }).encode()
    req = urllib.request.Request(LLM_URL, data=payload, headers={
        "Content-Type":"application/json",
        "Authorization": f"Bearer {LLM_KEY}",
        "X-Client-Id": CLIENT,
        "X-Clinic-Task": "soap_note",
    })
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            d = json.loads(r.read())
            return d["choices"][0]["message"]["content"]
    except Exception as e:
        return f"<ERR {e}>"

def extract_last_json_block(text):
    if not text or text.startswith("<"): return None
    text = re.sub(r"```(?:json)?", "", text).replace("```","")
    blocks=[]; depth=0; start=None; in_str=False; esc=False
    for i,c in enumerate(text):
        if in_str:
            if esc: esc=False
            elif c=="\\": esc=True
            elif c=='"': in_str=False
            continue
        if c=='"': in_str=True
        elif c=="{":
            if depth==0: start=i
            depth+=1
        elif c=="}":
            depth-=1
            if depth==0 and start is not None:
                blocks.append(text[start:i+1]); start=None
    for b in reversed(blocks):
        try: return json.loads(b)
        except Exception: continue
    return None

def run_one(sid, prompt_name, prompt):
    transcript = base64.b64decode(open(f"/tmp/sim_e2e/tr_{sid}.b64").read()).decode()[:32000]
    raw = call_llm(prompt, transcript)
    obj = extract_last_json_block(raw)
    if obj is None:
        return {"sid": sid, "prompt": prompt_name, "n_proc": None, "raw_excerpt": raw[:400]}
    procs = obj.get("procedure", [])
    cands = obj.get("procedure_candidates", [])
    return {
        "sid": sid, "prompt": prompt_name, "n_proc": len(procs),
        "procs": [{"action": p.get("action",""), "quote": (p.get("transcript_quote","") or "")[:120]} for p in procs],
        "candidates": [{"m": c.get("mention",""), "stance": c.get("stance",""), "q": (c.get("doctor_quote","") or "")[:80]} for c in cands],
    }

def main():
    results = []
    with ThreadPoolExecutor(max_workers=4) as ex:
        futures = []
        for sid in GROUND_TRUTH.keys():
            for pname, p in PROMPTS:
                futures.append(ex.submit(run_one, sid, pname, p))
        for f in as_completed(futures):
            r = f.result()
            results.append(r)
            print(f"  {r['prompt']:12} {r['sid']}  n_proc={r['n_proc']}", flush=True)

    # Score table
    print("\n\n=== Scoring ===")
    print(f"{'session':<10} {'truth':<6} " + " ".join(f"{p:<10}" for p,_ in PROMPTS))
    truth_map = {sid: gt for sid, (gt, _) in GROUND_TRUTH.items()}
    by_sid_pname = {(r['sid'], r['prompt']): r['n_proc'] for r in results}
    for sid, (gt, desc) in GROUND_TRUTH.items():
        truth = "PERF=1" if gt==1 else "NONE=0"
        cells = []
        for pname, _ in PROMPTS:
            n = by_sid_pname.get((sid, pname))
            if n is None:
                cells.append("ERR".ljust(10))
            elif (n>0) == (gt==1):
                cells.append(f"✓ {n}".ljust(10))
            else:
                cells.append(f"✗ {n}".ljust(10))
        print(f"{sid[:8]:<10} {truth:<6} " + " ".join(cells))
        print(f"           ({desc})")

    # Aggregate accuracy per prompt
    print("\n=== Aggregate accuracy per prompt ===")
    for pname, _ in PROMPTS:
        correct = 0; total = 0; tp=tn=fp=fn=0
        for sid, (gt, _) in GROUND_TRUTH.items():
            n = by_sid_pname.get((sid, pname))
            if n is None: continue
            total += 1
            if gt==1 and n>0: correct+=1; tp+=1
            elif gt==0 and n==0: correct+=1; tn+=1
            elif gt==0 and n>0: fp+=1
            elif gt==1 and n==0: fn+=1
        prec = tp/(tp+fp) if (tp+fp) else float('nan')
        rec  = tp/(tp+fn) if (tp+fn) else float('nan')
        print(f"  {pname:12} {correct}/{total} correct, precision={prec:.2f}, recall={rec:.2f}, TP={tp} TN={tn} FP={fp} FN={fn}")

    with open("/tmp/sim_e2e/prompt_iter_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("\nDetails written to /tmp/sim_e2e/prompt_iter_results.json")

if __name__ == "__main__":
    main()
