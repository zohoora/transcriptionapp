#!/usr/bin/env python3
"""Round 3: lock in V_VISIT_DX with two add-on riders to fix the
remaining persistent failures. Multi-seed variance (3 seeds).

Persistent fails in V_VISIT_DX:
  - Alexander Gulas: diabetic_assessment hallucination (both seeds)
  - Judith Croft: visit_type=intermediate (want virtual_phone)
  - Hasan Mirza: dx=300 anxiety vs ground-truth 724 back pain (1 seed)
  - Carol Fagan: visit=intermediate vs general_reassessment (1 seed)
"""

import json, urllib.request, base64, re, os
from concurrent.futures import ThreadPoolExecutor, as_completed
import importlib.util

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

STRICT_COND = """

--- STRICT CONDITION CHECK (R3 simulation) ---
Before emitting any K-code condition, verify these explicit requirements one more time:
- diabetic_assessment (K030A): REQUIRES the SOAP to explicitly mention DIABETES (type 1, type 2, GDM) AND specific diabetic management (A1C review, insulin/metformin/SGLT2 dose change FOR DIABETES, foot exam, glucose monitoring). DO NOT include if the SOAP only mentions arthritis, RA, Raynaud's, B12 injection, weight loss, Ozempic-for-weight, or family history of diabetes. If unsure, EXCLUDE.
- chf_management (Q050A): REQUIRES explicit heart failure / CHF / cardiomyopathy / reduced ejection fraction diagnosis AND HF-specific management. Bisoprolol or any beta-blocker for HTN alone is NOT chf_management.
- smoking_cessation (Q042A): REQUIRES the patient is a CURRENT TOBACCO smoker AND active counselling. Cannabis, marijuana, nasal spray cessation, alcohol — NOT smoking_cessation.

If your candidate condition's evidence in the SOAP doesn't meet ALL the requirements above, leave the conditions array empty for that condition.
"""

TELEPHONE_HINT = """

--- TELEPHONE / VIRTUAL VISIT DETECTION (R3 simulation) ---
If the SOAP's Objective section is EMPTY or contains only "Not documented" / patient-reported information (no physical exam findings, no vital signs measured today), strongly consider whether this was a TELEPHONE encounter:
- virtual_phone (A102A) — phone visit, no in-person exam.
- virtual_video (A101A) — video visit.

Empty Objective + scheduling/follow-up-only Plan + no procedures performed = likely a phone visit. Use visitType "virtual_phone" and setting "telephone_in_office" or "telephone_remote" accordingly.
"""

VARIANTS = [
    ("V_VISIT_DX",         V_BASE_PROMPT + VISIT_GUIDE + DX_COT,                              "soap_only", False),
    ("V_VISIT_DX_STRICT",  V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND,                "soap_only", False),
    ("V_VISIT_DX_PHONE",   V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND + TELEPHONE_HINT, "soap_only", False),
    ("V_VISIT_DX_BIND",    V_BASE_PROMPT + VISIT_GUIDE + DX_COT + STRICT_COND + TELEPHONE_HINT, "soap_only", True),
]

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
        if any(s in a for s in ("l1-","l2-","l3-","l4-","l5-","s1","lumbar","spine","sacral")) and "inject" in a:
            out.append("nerve_block_paravertebral"); continue
        if "trigger point" in a:
            out.append("trigger_point_injection"); continue
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
            print(f"  {r['variant']:20} {r['sid']} s={r['seed']}  v={r.get('visit_type','?')[:24]:<24} dx={r.get('dx','?'):<5} cond={r.get('conditions',[])}", flush=True)

    print("\n\n=== Round 3 scoring (per-instance, 3 seeds) ===")
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

    print(f"\n{'variant':<20} {'all_ok':<8} {'visit':<8} {'proc':<8} {'cond':<8} {'dx':<8}")
    for v, bv in by.items():
        n = bv["n"]
        print(f"{v:<20} {bv['all_ok']}/{n:<6} {bv['visit']}/{n:<6} {bv['proc']}/{n:<6} {bv['cond']}/{n:<6} {bv['dx']}/{n:<6}")

    # Majority-vote analysis: per session, what does each variant call?
    print("\n=== Majority-vote per session (3 seeds) ===")
    print(f"{'session':<14} " + "  ".join(f"{v:<20}" for v,_,_,_ in VARIANTS))
    for sid in GROUND_TRUTH:
        cells = []
        for vname,_,_,_ in VARIANTS:
            sub = [r for r in results if r["sid"]==sid and r["variant"]==vname and not r.get("err")]
            n_ok = sum(1 for r in sub if score(r, GROUND_TRUTH[sid])["all_ok"])
            cells.append(f"{n_ok}/{len(sub)}".ljust(20))
        print(f"{sid[:12]:<14}" + "  ".join(cells) + f"  ({GROUND_TRUTH[sid]['name'][:20]})")

    print("\n=== Hallucinations ===")
    for v, bv in by.items():
        if bv["hall"]:
            print(f"{v}: {bv['hall']}")

    with open("/tmp/sim_billing/round3_results.json","w") as f:
        json.dump(results, f, indent=2)

if __name__ == "__main__":
    main()
