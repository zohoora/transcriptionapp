#!/usr/bin/env python3
"""Round 2: drill into the two persistent failures.

Mary Beth (9b0b60e2): every prompt classified shoulder injection as PROPOSED
in candidates, yet still wrote procedure[] with 1 entry. The rule
'procedure = subset of candidates' isn't being self-enforced — try
hammering it harder.

Hasan (8feb77a1): no prompt finds the "here we go" mid-procedure cue.
Try variants that explicitly call out IN_PROGRESS narration markers."""

import json, urllib.request, base64, re
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
SOAP_MODEL = "soap-model-fast"
CLIENT  = "ai-scribe"

GROUND_TRUTH = {
    "0cce8568": (0, "Catherine Lamoureux"),
    "1ec7a57e": (0, "Alexander Gulas"),
    "9b0b60e2": (0, "Mary Beth Dop — modal/declined"),
    "b64ff7f3": (0, "Martin Gierling — declined"),
    "58e617ac": (1, "Linda Routledge — PERFORMED"),
    "8feb77a1": (1, "Hasan Mirza — PERFORMED ('here we go')"),
    "70e369bf": (1, "Apr 20 unknown — PERFORMED"),
    "173c4253": (1, "Helene Williamson — Plan: 'Proceed today'"),
}

# Round 2: 4 new variants

# V_STRICT: P_BEST but procedure[] derived ONLY from stance==COMPLETED
P_STRICT = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT EXACTLY THIS JSON:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_candidates":[{"mention":"<>","doctor_quote":"<>","stance":"PROPOSED|OFFERED|IN_PROGRESS|COMPLETED|DECLINED","walkback":"<>"}],"procedure":[{"action":"<>","transcript_quote":"<>"}]}

WORKFLOW (follow exactly):
1. Identify EVERY procedure mention. For each, fill procedure_candidates with stance:
   - PROPOSED: "I can inject", "we could try", "if you wanted", "would you like"
   - OFFERED: "I could spray with nitrogen, or use OTC"
   - IN_PROGRESS: "I'm putting alcohol on", "here we go", "hold still", "deep breath", "I just landmarked"
   - COMPLETED: doctor refers to it as DONE — past-tense action OR clear post-procedure narration ("the alcohol I just put on", "we drew the blood", "I sprayed it", "that's done", "the injection site is sore now")
   - DECLINED: "too early", "not today", "let's hold off", "use the OTC instead", "not worth it"
   - walkback: any later sentence contradicting performance ("instead", "let's wait", "maybe next visit"); "none" otherwise.

2. Build procedure[] — DETERMINISTIC FILTER:
   procedure[] = [{"action": c.mention rewritten in past tense, "transcript_quote": c.doctor_quote}
                  for c in procedure_candidates
                  if c.stance == "COMPLETED" AND c.walkback == "none"]

   IMPORTANT: a candidate with stance=PROPOSED, OFFERED, IN_PROGRESS, or DECLINED MUST NOT appear in procedure[]. EVER. Even if you think the procedure obviously happened.
   IMPORTANT: if multiple candidates refer to the SAME procedure and ANY of them has stance != COMPLETED, the procedure is excluded UNLESS at least one candidate for that same procedure has stance == COMPLETED with walkback == "none".

3. If no candidate has stance == COMPLETED, procedure[] MUST be the empty array [].

EXAMPLES:
  cand: stance=PROPOSED quote="I can inject the shoulder" → NOT in procedure[]
  cand: stance=DECLINED quote="too early to inject again" → NOT in procedure[]
  cand: stance=IN_PROGRESS quote="here we go" → NOT in procedure[] unless ALSO a COMPLETED candidate exists
  cand: stance=COMPLETED quote="the alcohol I just put on" → IN procedure[]
  cand: stance=COMPLETED walkback="use the OTC instead" → NOT in procedure[]

Output VALID JSON only. No prose."""


# V_NARR: emphasize in-progress narration markers (Hasan-style)
P_NARR = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT EXACTLY THIS JSON:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_candidates":[{"mention":"<>","doctor_quote":"<>","stance":"PROPOSED|OFFERED|IN_PROGRESS|COMPLETED|DECLINED","walkback":"<>","completion_evidence":"<>"}],"procedure":[{"action":"<>","transcript_quote":"<>"}]}

WORKFLOW:
1. Identify EVERY procedure mention. Classify each candidate's stance.
   - PROPOSED: modal/conditional ("I can", "we could", "if you wanted")
   - OFFERED: one option among several ("nitrogen, or OTC")
   - IN_PROGRESS: doctor narrating action mid-procedure. Look for ANY of these markers:
       * "here we go" / "here it goes" / "here goes"
       * "hold still" / "deep breath" / "ready?"
       * "I'm putting / placing / inserting / drawing / injecting [now/here]"
       * "I just landmarked" / "I'll put a bandaid"
       * "the alcohol" / "antiseptic" / "wipe" applied right before
   - COMPLETED: doctor refers to it as DONE. Look for:
       * "I sprayed / injected / drew / removed [it/that]"
       * "the alcohol I just put on" / "the injection site is sore" / "we drew the blood"
       * "all set" / "we're done" / "that's it"
   - DECLINED: explicit no ("too early", "not today", "use OTC instead", "let's hold off")
   - walkback: a sentence later in the transcript that says the procedure was NOT done after all
   - completion_evidence: a SECOND verbatim doctor quote (different from doctor_quote) that confirms the procedure FINISHED. If no such quote exists, set "none".

2. Build procedure[]: include ONLY candidates with stance == COMPLETED, OR stance == IN_PROGRESS WITH a non-"none" completion_evidence.
3. PROPOSED, OFFERED, DECLINED candidates NEVER enter procedure[].
4. If walkback != "none", exclude that procedure.

Output VALID JSON only. No prose."""


# V_TWO_STAGE: "Did this actually happen?" verification
P_TWO = """You are a medical scribe that outputs ONLY valid JSON.

OUTPUT EXACTLY THIS JSON:
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure_audit":[{"procedure":"<>","first_mention":"<>","did_it_happen":"YES|NO|UNCLEAR","completion_quote_or_reason":"<>"}],"procedure":[{"action":"<>","transcript_quote":"<>"}]}

WORKFLOW (be conservative):
1. Identify every procedure CANDIDATE in the transcript. For each, fill procedure_audit:
   - first_mention: the first relevant doctor quote
   - did_it_happen: be conservative
       * YES — only if you can quote the doctor describing the procedure as DONE/COMPLETED in past tense, OR describing post-procedure care that clearly refers to today's procedure
       * NO — if the doctor proposed/offered but explicitly declined, walked back, or recommended an alternative
       * UNCLEAR — if the transcript only contains modal/future/conditional language with no completion confirmation
   - completion_quote_or_reason: if YES, paste the verbatim completion quote. If NO/UNCLEAR, briefly explain.

2. Build procedure[]: include ONLY entries where did_it_happen == "YES".
3. UNCLEAR is treated as NO — DO NOT include in procedure[].

CONSERVATIVE PRINCIPLE: when uncertain, choose UNCLEAR / NO. False billing is far worse than missed billing — clinicians can manually add a missed procedure but auto-charged hallucinations cause OHIP claw-backs.

Output VALID JSON only."""


# V_NARR_PLUS: combine STRICT filter + NARR markers
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


PROMPTS = [
    ("P_STRICT",    P_STRICT),
    ("P_NARR",      P_NARR),
    ("P_TWO",       P_TWO),
    ("P_NARR_PLUS", P_NARR_PLUS),
]

def call_llm(system, user, timeout=120):
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

def run_one(sid, pname, prompt):
    transcript = base64.b64decode(open(f"/tmp/sim_e2e/tr_{sid}.b64").read()).decode()[:32000]
    raw = call_llm(prompt, transcript)
    obj = extract_last_json_block(raw)
    if obj is None:
        return {"sid":sid,"prompt":pname,"n_proc":None,"raw":raw[:300]}
    return {
        "sid":sid,"prompt":pname,
        "n_proc":len(obj.get("procedure",[])),
        "procs":[{"action":p.get("action",""),"quote":(p.get("transcript_quote","") or "")[:120]} for p in obj.get("procedure",[])],
        "candidates":[{"m":c.get("mention",""),"stance":c.get("stance","") or c.get("did_it_happen",""),"q":(c.get("doctor_quote","") or c.get("first_mention","") or "")[:80]} for c in (obj.get("procedure_candidates",[]) or obj.get("procedure_audit",[]))],
    }

def main():
    results=[]
    with ThreadPoolExecutor(max_workers=4) as ex:
        futs=[ex.submit(run_one, sid, pname, p) for sid in GROUND_TRUTH for pname,p in PROMPTS]
        for f in as_completed(futs):
            r = f.result()
            results.append(r)
            print(f"  {r['prompt']:14} {r['sid']}  n_proc={r['n_proc']}", flush=True)

    print("\n\n=== Scoring ===")
    print(f"{'session':<10} {'truth':<6} " + " ".join(f"{p:<14}" for p,_ in PROMPTS))
    by={(r['sid'],r['prompt']):r['n_proc'] for r in results}
    for sid,(gt,desc) in GROUND_TRUTH.items():
        truth = "PERF=1" if gt==1 else "NONE=0"
        cells=[]
        for pname,_ in PROMPTS:
            n = by.get((sid,pname))
            if n is None: cells.append("ERR".ljust(14))
            elif (n>0)==(gt==1): cells.append(f"✓ {n}".ljust(14))
            else: cells.append(f"✗ {n}".ljust(14))
        print(f"{sid[:8]:<10} {truth:<6} " + " ".join(cells))
        print(f"           ({desc})")

    print("\n=== Aggregate ===")
    for pname,_ in PROMPTS:
        tp=tn=fp=fn=0
        for sid,(gt,_) in GROUND_TRUTH.items():
            n = by.get((sid,pname))
            if n is None: continue
            if gt==1 and n>0: tp+=1
            elif gt==0 and n==0: tn+=1
            elif gt==0 and n>0: fp+=1
            elif gt==1 and n==0: fn+=1
        prec = tp/(tp+fp) if (tp+fp) else float('nan')
        rec  = tp/(tp+fn) if (tp+fn) else float('nan')
        print(f"  {pname:14} {tp+tn}/8 correct, precision={prec:.2f}, recall={rec:.2f}, TP={tp} TN={tn} FP={fp} FN={fn}")

    with open("/tmp/sim_e2e/iter2_results.json","w") as f: json.dump(results,f,indent=2)

if __name__ == "__main__":
    main()
