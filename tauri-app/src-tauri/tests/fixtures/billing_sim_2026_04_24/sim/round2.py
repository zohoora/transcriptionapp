#!/usr/bin/env python3
"""Round 2: address Round 1's dominant errors.

Round 1 leader: V_SOAP_ONLY (9/26). Failure clusters:
- Visit type drift (intermediate vs general_reassessment vs virtual_phone)
- Dx code wrong family (715 vs 714, 729 vs 724, 401 vs 707, 785 vs 216)
- V_PROC_BIND regex too narrow (Linda's "L4-5" not matched)

Round 2 variants:
- V_BASE_R1   : V_SOAP_ONLY (Round 1 winner) — control
- V_VISIT     : V_SOAP_ONLY + visit-type calibration guide (multi-issue→A004, telephone→A102, etc.)
- V_DX_COT    : V_SOAP_ONLY + dx chain-of-thought (list all dx, identify chief, then code)
- V_VISIT_DX  : combine V_VISIT + V_DX_COT
- V_PROC_BIND2: improved mapping regex; tests deterministic procedure binding
"""

import json, urllib.request, base64, re, os
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
BILL_MODEL = "fast-model"
CLIENT  = "ai-scribe"
SEEDS = 2

import importlib.util
spec = importlib.util.spec_from_file_location("r1", "/tmp/sim_billing/round1.py")
r1 = importlib.util.module_from_spec(spec); spec.loader.exec_module(r1)
GROUND_TRUTH = r1.GROUND_TRUTH

V_BASE_PROMPT = open("/tmp/sim_billing/billing_current_prompt.txt").read()

VISIT_GUIDE = """

--- VISIT TYPE CALIBRATION GUIDE (R2 simulation) ---
Carefully calibrate visitType to the actual scope of the visit:
- "minor_assessment" (A001A) — SINGLE focused complaint, brief history + targeted exam, <10 min. UTI symptom check, single Rx renewal with brief exam, single rash review.
- "intermediate_assessment" (A007A) — Moderate complexity, 1-2 issues, 10-20 min. Standard follow-up visits, well-baby checks, routine chronic disease follow-up.
- "general_reassessment" (A004A) — Comprehensive ESTABLISHED patient follow-up addressing MULTIPLE active problems (3+). Multi-system review, typically 20-30 min. If the SOAP has many distinct A-section diagnoses or many medication discussions, prefer A004A over A007A.
- "general_assessment" (A003A) — Comprehensive NEW patient workup OR annual complete exam.
- "mini_assessment" (A008A) — <5 min, no exam (Rx renewal, form signature only).
- "virtual_phone" (A102A) — Telephone visit. SIGNALS: SOAP starts with "Hi [name], it's Doctor X calling", phrases like "we'll call you back", the visit is over the phone (no in-person exam findings).
- "virtual_video" (A101A) — Video telemedicine visit.

Pick the visit type that BEST matches the SOAP scope. Default to A007A (intermediate) only when 1-2 issues; if the SOAP discusses 3+ distinct problems, use A004A.
"""

DX_COT = """

--- DIAGNOSTIC CODE REASONING (R2 simulation) ---
Before emitting suggestedDiagnosticCode, follow this internal reasoning:
1. List ALL distinct diagnoses in the SOAP A section.
2. Identify the CHIEF COMPLAINT — the primary reason the patient came in today (often the dominant entry in S, or the diagnosis that drove the most active management in P).
3. Pick the 3-digit OHIP code that matches the CHIEF COMPLAINT, NOT the most prominent chronic condition.

Examples:
- SOAP: "S: nasal congestion. A: rhinitis medicamentosa. P: stop nasal spray." Chief = nasal congestion → 477 (allergic rhinitis).
- SOAP: "S: wart on foot. A: HTN management on apixaban. Plantar wart." Chief = wart (visit reason) → 707 (skin lesion). HTN (401) is incidental.
- SOAP: "S: foot swelling. A: foot edema, pending DVT rule-out." Chief = foot swelling → 451 (phlebitis/DVT).
- SOAP: "S: skin tags. A: irritated acrochordons. P: liquid nitrogen offered." Chief = skin tags → 216 (benign skin neoplasm).
- SOAP: "S: shoulder pain. A: rotator cuff tendinopathy AND type 2 diabetes (med refills)." Chief = shoulder pain → 715 (osteoarthritis) or 727 (synovitis). NOT 250 (diabetes).
- SOAP: "S: low back pain x 1 week. A: chronic LBP with sciatica." Chief = low back pain → 724.
- SOAP: "A: rheumatoid arthritis with peripheral nerve symptoms." Chief = RA management → 714 (RA), NOT 715 (osteoarthritis).

Output the chief-complaint dx code in suggestedDiagnosticCode. If multiple dx codes are equally plausible, pick the most specific one tied to the visit's reason for visit.
"""

VARIANTS = [
    ("V_BASE_R1",   V_BASE_PROMPT,                                   "soap_only", False),
    ("V_VISIT",     V_BASE_PROMPT + VISIT_GUIDE,                     "soap_only", False),
    ("V_DX_COT",    V_BASE_PROMPT + DX_COT,                          "soap_only", False),
    ("V_VISIT_DX",  V_BASE_PROMPT + VISIT_GUIDE + DX_COT,            "soap_only", False),
    ("V_PROC_BIND2",V_BASE_PROMPT + VISIT_GUIDE + DX_COT,            "soap_only", True),
]

# Improved regex map for V_PROC_BIND2
def soap_procedure_bind(sid):
    soap_obj = json.load(open(f"/tmp/sim_billing/soap_v061_{sid}.json"))
    procs = soap_obj.get("procedure") or []
    out = []
    for p in procs:
        a = (p.get("action","") or "").lower()
        if "cryo" in a or "liquid nitrogen" in a or "frozen" in a or "froze " in a:
            out.append("cryotherapy_single"); continue
        if "epidural" in a or "paravertebral" in a:
            out.append("nerve_block_paravertebral"); continue
        # Lumbar / spinal injection (L1..L5, S1, lumbar, spine, sacral)
        if any(s in a for s in ("l1-","l2-","l3-","l4-","l5-","s1","lumbar","spine","sacral")) and "inject" in a:
            out.append("nerve_block_paravertebral"); continue
        if "trigger point" in a:
            out.append("trigger_point_injection"); continue
        # Joint injection (knee, shoulder, hip, etc.) — must contain "joint" or specific named joint
        if "joint inject" in a or any(s in a for s in ("knee inject","shoulder inject","hip inject","cortisone"," prp ")) and "inject" in a:
            out.append("joint_injection"); continue
        if "blood draw" in a or "venipuncture" in a or "drew blood" in a:
            out.append("im_injection_with_visit"); continue
        if "im injection" in a or "intramuscular" in a or "im/sc" in a or "b12 injection" in a:
            out.append("im_injection_with_visit"); continue
    return out

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
    procs = obj.get("procedures") or []
    if post_bind:
        procs = soap_procedure_bind(sid)
    return {
        "sid": sid, "variant": vname, "seed": seed,
        "visit_type": obj.get("visitType"),
        "procedures": procs,
        "conditions": obj.get("conditions") or [],
        "dx": obj.get("suggestedDiagnosticCode") or "",
        "primary_dx_text": (obj.get("primaryDiagnosis") or "")[:80],
    }

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
            print(f"  {r['variant']:14} {r['sid']} s={r['seed']}  v={r.get('visit_type','?')[:24]:<24} dx={r.get('dx','?'):<5} procs={r.get('procedures',[])}", flush=True)

    print("\n\n=== Round 2 scoring ===")
    by_variant = {}
    for r in results:
        if r.get("err"): continue
        gt = GROUND_TRUTH[r["sid"]]
        s = r1.score_one(r, gt)
        bv = by_variant.setdefault(r["variant"], {"all_ok":0,"visit":0,"proc":0,"cond":0,"dx":0,"n":0,"hall":[]})
        bv["n"] += 1
        if s["all_ok"]: bv["all_ok"] += 1
        if s["visit_ok"]: bv["visit"] += 1
        if s["proc_ok"]: bv["proc"] += 1
        if s["cond_ok"]: bv["cond"] += 1
        if s["dx_ok"]: bv["dx"] += 1
        if s["cond_hallucinated"]: bv["hall"].append((r["sid"], s["cond_hallucinated"]))

    print(f"\n{'variant':<14} {'all_ok':<8} {'visit':<8} {'proc':<8} {'cond':<8} {'dx':<8}")
    for vname, bv in by_variant.items():
        n = bv["n"]
        print(f"{vname:<14} {bv['all_ok']}/{n:<6} {bv['visit']}/{n:<6} {bv['proc']}/{n:<6} {bv['cond']}/{n:<6} {bv['dx']}/{n:<6}")

    print("\n=== Hallucinations ===")
    for vname, bv in by_variant.items():
        if bv["hall"]:
            print(f"{vname}: {bv['hall']}")

    with open("/tmp/sim_billing/round2_results.json","w") as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()
