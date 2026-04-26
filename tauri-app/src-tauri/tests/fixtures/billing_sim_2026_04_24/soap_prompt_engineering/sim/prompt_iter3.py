#!/usr/bin/env python3
"""Round 3: lock in the winner.

Round 2 P_NARR_PLUS was best (6/8 or 6/7 excluding Martin's transcript-vs-memory ambiguity).
Two remaining issues:
1. Hasan timed out / truncated on the wider-output prompt. Simplify output schema.
2. Test variance across multiple seeds to make sure the winner is stable.

Martin's transcript ACTUALLY shows cryo was applied ("I know this is painful. I'm sorry" +
"you are done, my friend"). Clinician's written feedback said "only discussed" —
transcript disagrees. We exclude Martin from scoring (no LLM-only approach can
match clinician's memory when transcript contradicts it).
"""

import json, urllib.request, base64, re, time
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
SOAP_MODEL = "soap-model-fast"
CLIENT  = "ai-scribe"

# Martin excluded — transcript shows cryo applied despite clinician feedback.
GROUND_TRUTH = {
    "0cce8568": (0, "Catherine Lamoureux"),
    "1ec7a57e": (0, "Alexander Gulas"),
    "9b0b60e2": (0, "Mary Beth Dop — modal/declined"),
    "58e617ac": (1, "Linda Routledge — PERFORMED"),
    "8feb77a1": (1, "Hasan Mirza — 'here we go'"),
    "70e369bf": (1, "Apr 20 — SOAP O explicit"),
    "173c4253": (1, "Helene Williamson — 'Proceed today'"),
}

# Winner from round 2 — keep as V_A
P_NARR_PLUS = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT EXACTLY THIS JSON:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_candidates":[{"mention":"<>","doctor_quote":"<>","stance":"PROPOSED|OFFERED|IN_PROGRESS|COMPLETED|DECLINED","walkback":"<>","completion_evidence":"<>"}],"procedure":[{"action":"<>","transcript_quote":"<>"}]}

WORKFLOW:
1. Identify EVERY procedure mention. Classify stance:
   - PROPOSED: "I can inject", "we could try", "if you wanted", "would you like" — modal capability
   - OFFERED: "I could spray with nitrogen, or use OTC" — one option among several
   - IN_PROGRESS: doctor narrating action mid-procedure. Markers include:
       "here we go", "here it goes", "hold still", "deep breath", "ready?",
       "I'm putting/placing/inserting/drawing/injecting now",
       "the alcohol" or "antiseptic" being applied right before,
       "I just landmarked", "I'll put a bandaid"
   - COMPLETED: clear past-tense or aftercare narration:
       "I sprayed/injected/drew/removed it", "the alcohol I just put on",
       "the injection site is sore", "we drew the blood", "all set", "we're done"
   - DECLINED: "too early", "not today", "let's hold off", "use OTC instead", "not worth it"
   - walkback: later sentence contradicting performance
   - completion_evidence: a SECOND quote confirming the procedure FINISHED, or "none"

2. DETERMINISTIC FILTER for procedure[]:
   for each candidate c:
     if c.walkback != "none": skip
     if c.stance == "COMPLETED": include
     elif c.stance == "IN_PROGRESS" and c.completion_evidence != "none": include
     else: skip

3. PROPOSED, OFFERED, DECLINED candidates NEVER enter procedure[]. Even if you think the procedure obviously happened.

4. If multiple candidates refer to the SAME procedure, evaluate together — only include if at least ONE meets the inclusion rule above.

Output VALID JSON only."""


# V_B: narrower output (maybe the wider schema caused Hasan timeout)
P_SLIM = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT EXACTLY:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"<past-tense>","stance_check":"COMPLETED|IN_PROGRESS","quote":"<verbatim doctor quote proving completion>","walkback":"<verbatim contradicting quote or 'none'>"}]}

RULES for building procedure[]:
- INCLUDE only if the doctor's speech contains a COMPLETED action (past-tense "I sprayed / injected / drew", or aftercare "the injection site is sore", "the alcohol I just put on", "we drew the blood") OR an IN_PROGRESS narration ("here we go", "hold still", "I'm injecting now") followed by completion.
- EXCLUDE modal / future / conditional: "I can inject", "let me grab", "we could", "would you like", "I'll", "we'll", "schedule", "arrange".
- EXCLUDE if doctor later walks back or recommends alternative: "instead", "OTC", "let's wait", "not today", "too early".

IF UNCERTAIN: exclude. Empty procedure[] is the right answer when evidence is ambiguous.

S/O/A/P sections: standard. Do NOT mirror Plan items into procedure[] unless completion is quoted.

Output VALID JSON only."""


# V_C: even simpler — single reasoning field, conservative default
P_CONSERVATIVE = """You are a medical scribe that outputs ONLY valid JSON:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[]}

PROCEDURE section — STRICT:
Include a procedure in the procedure[] array ONLY when the transcript contains verbatim doctor speech proving the procedure was COMPLETED today. Acceptable evidence:
  • Past-tense doctor action: "I sprayed the wart", "I just injected the knee", "I drew the blood"
  • Post-procedure narration: "the alcohol I just put on", "the injection site is sore", "we're done", "all set"
  • Mid-procedure narration clearly tied to completion: "hold still" / "here we go" followed by aftercare speech

NEVER include a procedure when the transcript only contains:
  • Modal capability: "I can inject", "we could spray"
  • Future intent: "I'll / we'll / going to / let me grab"
  • Conditional offers: "if you wanted", "would you like", "we could"
  • Scheduling: "schedule", "arrange", "next visit"
  • Explicit decline: "too early", "not today", "let's hold off", "use OTC instead"

BIAS: When in doubt, leave procedure[] EMPTY. A missed procedure can be added manually; a wrongly-charged procedure risks an OHIP claw-back.

Each procedure[] entry: {"action": "<past-tense>", "transcript_quote": "<verbatim doctor quote proving completion>"}

Output VALID JSON only."""


PROMPTS = [
    ("P_NARR_PLUS",    P_NARR_PLUS),
    ("P_SLIM",         P_SLIM),
    ("P_CONSERVATIVE", P_CONSERVATIVE),
]

SEEDS = 3  # run each combination 3x to measure variance

def call_llm(system, user, timeout=180):
    payload = json.dumps({"model":SOAP_MODEL,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"temperature":0.1}).encode()
    req = urllib.request.Request(LLM_URL, data=payload, headers={"Content-Type":"application/json","Authorization":f"Bearer {LLM_KEY}","X-Client-Id":CLIENT,"X-Clinic-Task":"soap_note"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read())["choices"][0]["message"]["content"]
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

def run_one(sid, pname, prompt, seed):
    transcript = base64.b64decode(open(f"/tmp/sim_e2e/tr_{sid}.b64").read()).decode()[:32000]
    raw = call_llm(prompt, transcript)
    obj = extract_last_json_block(raw)
    if obj is None:
        return {"sid":sid,"prompt":pname,"seed":seed,"n_proc":None}
    return {"sid":sid,"prompt":pname,"seed":seed,"n_proc":len(obj.get("procedure",[]))}

def main():
    results=[]
    with ThreadPoolExecutor(max_workers=2) as ex:
        futs=[]
        for sid in GROUND_TRUTH:
            for pname,p in PROMPTS:
                for s in range(SEEDS):
                    futs.append(ex.submit(run_one, sid, pname, p, s))
        for f in as_completed(futs):
            r = f.result()
            results.append(r)
            print(f"  {r['prompt']:16} {r['sid']} seed={r['seed']}  n_proc={r['n_proc']}", flush=True)

    # Aggregate — vote across seeds (majority n_proc >0 → performed)
    print("\n\n=== Scoring (majority vote across 3 seeds) ===")
    print(f"{'session':<10} {'truth':<6} " + " ".join(f"{p:<18}" for p,_ in PROMPTS))
    by_sid_p = {}
    for r in results:
        by_sid_p.setdefault((r['sid'],r['prompt']),[]).append(r['n_proc'])
    for sid,(gt,desc) in GROUND_TRUTH.items():
        truth = "PERF=1" if gt==1 else "NONE=0"
        cells=[]
        for pname,_ in PROMPTS:
            seeds_ = by_sid_p.get((sid,pname),[])
            valid = [x for x in seeds_ if x is not None]
            if not valid:
                cells.append("ERR".ljust(18))
                continue
            positives = sum(1 for x in valid if x>0)
            # Majority call
            called = 1 if positives > len(valid)/2 else 0
            detail = f"[{'/'.join(str(x) if x is not None else '?' for x in seeds_)}]"
            mark = "✓" if (called>0)==(gt==1) else "✗"
            cells.append(f"{mark} {called} {detail}".ljust(18))
        print(f"{sid[:8]:<10} {truth:<6} " + " ".join(cells))
        print(f"           ({desc})")

    print("\n=== Aggregate per prompt (majority vote) ===")
    for pname,_ in PROMPTS:
        tp=tn=fp=fn=0
        for sid,(gt,_) in GROUND_TRUTH.items():
            seeds_ = by_sid_p.get((sid,pname),[])
            valid = [x for x in seeds_ if x is not None]
            if not valid: continue
            positives = sum(1 for x in valid if x>0)
            called = 1 if positives > len(valid)/2 else 0
            if gt==1 and called==1: tp+=1
            elif gt==0 and called==0: tn+=1
            elif gt==0 and called==1: fp+=1
            else: fn+=1
        prec = tp/(tp+fp) if (tp+fp) else float('nan')
        rec  = tp/(tp+fn) if (tp+fn) else float('nan')
        n = tp+tn+fp+fn
        print(f"  {pname:16} {tp+tn}/{n} majority-correct, precision={prec:.2f}, recall={rec:.2f}, TP={tp} TN={tn} FP={fp} FN={fn}")

    with open("/tmp/sim_e2e/iter3_results.json","w") as f: json.dump(results,f,indent=2)

if __name__ == "__main__":
    main()
