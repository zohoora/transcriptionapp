#!/usr/bin/env python3
"""Round 4: refine STRICT — recover dx accuracy, narrower phone signal,
add evidence-quote requirement for diabetic_assessment.

Round 3 picture:
- V_VISIT_DX             30/39 — best all_ok, but Alexander hallucinates 3/3
- V_VISIT_DX_STRICT      28/39 — zero hallucinations, but Hasan dx drops
- V_VISIT_DX_PHONE       26/39 — recovers Judith but breaks Catherine + Carol
- V_VISIT_DX_BIND        24/39 — worse

Round 4 tries to combine the best of these.
"""

import json, urllib.request, base64, re, os, importlib.util
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
BILL_MODEL = "fast-model"
CLIENT  = "ai-scribe"
SEEDS = 3

spec = importlib.util.spec_from_file_location("r1", "/tmp/sim_billing/round1.py")
r1 = importlib.util.module_from_spec(spec); spec.loader.exec_module(r1)
GROUND_TRUTH = r1.GROUND_TRUTH

V_BASE_PROMPT = open("/tmp/sim_billing/billing_current_prompt.txt").read()
VISIT_GUIDE = open("/tmp/sim_billing/round2.py").read().split('VISIT_GUIDE = """', 1)[1].split('"""', 1)[0]
DX_COT = open("/tmp/sim_billing/round2.py").read().split('DX_COT = """', 1)[1].split('"""', 1)[0]
STRICT_COND = open("/tmp/sim_billing/round3.py").read().split('STRICT_COND = """', 1)[1].split('"""', 1)[0]

# Soft phone hint — only fires when SOAP explicitly mentions phone/call markers
SOFT_PHONE = """

--- TELEPHONE VISIT — NARROW SIGNAL (R4) ---
Use visitType "virtual_phone" (A102A) ONLY when the SOAP contains explicit telephone-call language. Look for these markers ANYWHERE in S/O/A/P:
  • "phone call" / "calling" / "we'll call" / "we will call" / "called the patient"
  • "telephone visit" / "telephone consultation" / "phone visit"
  • Patient interaction described as "over the phone" / "by phone"

If NONE of these markers appear, do NOT pick virtual_phone. Empty Objective alone is NOT sufficient — many in-office follow-ups (chronic disease check, post-op review) also have minimal Objective and are still in-office visits."""

# Evidence-quote requirement for diabetic_assessment specifically
DIAB_EVIDENCE = """

--- DIABETIC_ASSESSMENT EVIDENCE QUOTE (R4) ---
You may include "diabetic_assessment" in conditions ONLY IF you can quote the EXACT WORD "diabetes" or "diabetic" or "type 1 DM" or "type 2 DM" or "T2DM" or "T1DM" or "GDM" from the SOAP A section. Provide the exact quote in conditionEvidence as: "Quoted from SOAP A: '<verbatim>'".

If the SOAP A does NOT contain any of those exact terms, you MUST NOT include diabetic_assessment — even if the SOAP mentions Ozempic, B12 injections, weight management, RA, or Raynaud's phenomenon. These conditions can have similar appearances but are NOT diabetes.

If unsure, leave conditions empty for diabetic_assessment. False positives cause OHIP claw-backs."""

VARIANTS = [
    ("V_R3_BEST",    V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND,                                  "soap_only", False),
    ("V_R4_PHONE",   V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND + SOFT_PHONE,                     "soap_only", False),
    ("V_R4_DIAB",    V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND + DIAB_EVIDENCE,                  "soap_only", False),
    ("V_R4_FULL",    V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND + SOFT_PHONE + DIAB_EVIDENCE,     "soap_only", False),
]

def call(system, user, timeout=120):
    payload = json.dumps({"model":BILL_MODEL,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"temperature":0.1}).encode()
    req = urllib.request.Request(LLM_URL, data=payload, headers={
        "Content-Type":"application/json","Authorization":f"Bearer {LLM_KEY}",
        "X-Client-Id":CLIENT,"X-Clinic-Task":"billing_extraction"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read())["choices"][0]["message"]["content"]
    except Exception as e:
        return f"<ERR {e}>"

def run_one(sid, vname, system, mode, post_bind, seed):
    user = r1.build_user(sid, mode)
    raw = call(system, user)
    obj = r1.extract_last_json_block(raw)
    if obj is None:
        return {"sid":sid,"variant":vname,"seed":seed,"err":True}
    return {
        "sid": sid, "variant": vname, "seed": seed,
        "visit_type": obj.get("visitType"),
        "procedures": obj.get("procedures") or [],
        "conditions": obj.get("conditions") or [],
        "dx": obj.get("suggestedDiagnosticCode") or "",
    }

def score(r, gt):
    s = {}
    s["visit_ok"] = (gt.get("visit_type") is None) or (r.get("visit_type") == gt.get("visit_type"))
    expected = set(gt["procedures"]); optional = set(gt.get("procedures_optional", set()))
    actual = set(r["procedures"])
    if not expected:
        s["proc_ok"] = actual.issubset(optional)
    else:
        s["proc_ok"] = bool(actual) and actual.issubset(expected | optional) and bool(actual & expected)
    forbidden = set(gt["forbidden_conditions"])
    bad = [c for c in r["conditions"] if c in forbidden]
    s["cond_hallucinated"] = bad
    s["cond_ok"] = not bad
    s["dx_ok"] = (r.get("dx") in gt["ok_dx"]) if gt["ok_dx"] else True
    s["all_ok"] = s["visit_ok"] and s["proc_ok"] and s["cond_ok"] and s["dx_ok"]
    return s

def main():
    tasks = []
    for sid in GROUND_TRUTH:
        if not os.path.exists(f"/tmp/sim_billing/soap_v061_{sid}.json"):
            continue
        for vname, system, mode, post_bind in VARIANTS:
            for seed in range(SEEDS):
                tasks.append((sid, vname, system, mode, post_bind, seed))
    print(f"Total LLM calls: {len(tasks)}", flush=True)

    results = []
    with ThreadPoolExecutor(max_workers=2) as ex:
        futs = {ex.submit(run_one, *t): t for t in tasks}
        for f in as_completed(futs):
            r = f.result()
            results.append(r)
            print(f"  {r['variant']:14} {r['sid']} s={r['seed']}  v={r.get('visit_type','?')[:24]:<24} dx={r.get('dx','?'):<5} cond={r.get('conditions',[])}", flush=True)

    print("\n=== Round 4 scoring ===")
    by = {}
    for r in results:
        if r.get("err"): continue
        s = score(r, GROUND_TRUTH[r["sid"]])
        bv = by.setdefault(r["variant"], {"all_ok":0,"visit":0,"proc":0,"cond":0,"dx":0,"n":0,"hall":[]})
        bv["n"] += 1
        if s["all_ok"]: bv["all_ok"] += 1
        if s["visit_ok"]: bv["visit"] += 1
        if s["proc_ok"]: bv["proc"] += 1
        if s["cond_ok"]: bv["cond"] += 1
        if s["dx_ok"]: bv["dx"] += 1
        if s["cond_hallucinated"]: bv["hall"].append((r["sid"], r["seed"], s["cond_hallucinated"]))

    print(f"\n{'variant':<14} {'all_ok':<8} {'visit':<8} {'proc':<8} {'cond':<8} {'dx':<8}")
    for v, bv in by.items():
        n = bv["n"]
        print(f"{v:<14} {bv['all_ok']}/{n:<6} {bv['visit']}/{n:<6} {bv['proc']}/{n:<6} {bv['cond']}/{n:<6} {bv['dx']}/{n:<6}")

    print("\n=== Per-session majority vote ===")
    print(f"{'session':<14} " + "  ".join(f"{v:<14}" for v,_,_,_ in VARIANTS))
    for sid in GROUND_TRUTH:
        cells = []
        for vname,_,_,_ in VARIANTS:
            sub = [r for r in results if r["sid"]==sid and r["variant"]==vname and not r.get("err")]
            n_ok = sum(1 for r in sub if score(r, GROUND_TRUTH[sid])["all_ok"])
            cells.append(f"{n_ok}/{len(sub)}".ljust(14))
        print(f"{sid[:12]:<14}" + "  ".join(cells) + f"  ({GROUND_TRUTH[sid]['name'][:20]})")

    print("\n=== Hallucinations ===")
    for v, bv in by.items():
        if bv["hall"]:
            print(f"{v}: {bv['hall']}")

    with open("/tmp/sim_billing/round4_results.json","w") as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()
