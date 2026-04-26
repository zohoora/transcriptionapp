#!/usr/bin/env python3
"""Round 1: 5 billing-prompt variants × 13 sessions × 2 seeds.

Scored against this week's ground truth (visit_type, procedures, conditions, dx).
"""

import json, urllib.request, base64, re, os
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
BILL_MODEL = "fast-model"
CLIENT  = "ai-scribe"

# ── Ground truth ──────────────────────────────────────────────────────────
# For each session: (visit_type, set(procedures), set(forbidden_conditions),
# acceptable_dx_codes set, notes).
# - "forbidden_conditions" are condition enum values that MUST NOT appear
#   (the hallucinations from this week).
# - acceptable_dx_codes = a set of 3-digit codes any of which is OK; 799
#   fallback is acceptable only when no other code is plausible.
GROUND_TRUTH = {
    "0cce8568": dict(name="Catherine Lamoureux",
        visit_type="minor_assessment",
        procedures=set(),
        forbidden_conditions={"diabetic_assessment","chf_management","smoking_cessation"},
        ok_dx={"599","788"},
        notes="UTI symptoms, urine sample collection only — no procedure"),
    "de7f5122": dict(name="Carol Fagan",
        visit_type="general_reassessment",
        procedures=set(),
        forbidden_conditions=set(),
        ok_dx={"725","714"},
        notes="PMR follow-up — production was correct"),
    "491c6493": dict(name="Cody Milmine",
        visit_type=None,  # accept intermediate or general_reassessment
        procedures=set(),
        forbidden_conditions={"smoking_cessation"},  # cannabis NOT tobacco
        ok_dx={"311","300"},
        notes="ADHD/depression/cannabis — smoking_cessation is hallucination"),
    "e11d3112": dict(name="Catherine Grieve",
        visit_type="general_reassessment",
        procedures=set(),
        forbidden_conditions={"diabetic_assessment"},
        ok_dx={"311","300","692"},
        notes="Acne + anxiety/PTSD — no procedure"),
    "58e617ac": dict(name="Linda Routledge",
        visit_type=None,  # accept intermediate or general_reassessment
        procedures={"nerve_block_paravertebral","joint_injection"},  # either acceptable
        forbidden_conditions=set(),
        ok_dx={"724","722"},
        notes="L4-5 paravertebral injection performed"),
    "173c4253": dict(name="Helene Williamson",
        visit_type=None,
        procedures={"nerve_block_paravertebral","joint_injection"},
        forbidden_conditions=set(),
        ok_dx={"724"},
        notes="Epidural at L4-L5 today"),
    "8feb77a1": dict(name="Hasan Mirza",
        visit_type=None,
        procedures=set(),  # ambiguous — empty acceptable
        procedures_optional={"nerve_block_paravertebral","joint_injection"},
        forbidden_conditions=set(),
        ok_dx={"724"},
        notes="Discussion-heavy; either empty or nerve_block acceptable"),
    "1ec7a57e": dict(name="Alexander Gulas",
        visit_type=None,
        procedures=set(),
        procedures_optional={"im_injection_with_visit"},  # B12 was scheduled
        forbidden_conditions={"diabetic_assessment","chf_management","smoking_cessation"},
        ok_dx={"714"},
        notes="RA flare — NO diabetes management"),
    "ccc2b9fb": dict(name="Chris Miles",
        visit_type="general_reassessment",
        procedures=set(),
        forbidden_conditions={"diabetic_assessment","chf_management"},
        ok_dx={"715","724"},
        notes="Multi-joint OA review — Ozempic for WEIGHT not diabetes"),
    "bc81d25e": dict(name="Judith Croft",
        visit_type="virtual_phone",
        procedures=set(),
        forbidden_conditions=set(),
        ok_dx={"451","785","459","786"},  # foot swelling / DVT rule-out
        notes="Telephone visit, foot swelling DVT workup — 799 unacceptable"),
    "eeb4d85f": dict(name="Jerry Zandbergen",
        visit_type=None,
        procedures=set(),
        forbidden_conditions={"diabetic_assessment","chf_management","smoking_cessation"},
        ok_dx={"401","599"},
        notes="HTN management — NO diabetes"),
    "98821f67": dict(name="Louise Simon (mislabeled Jerry)",
        visit_type=None,
        procedures=set(),
        procedures_optional={"cryotherapy_single"},  # transcript shows offered
        forbidden_conditions=set(),
        ok_dx={"216","709","692","786"},
        notes="Skin tags + ear discharge + chest wall pain — multi-issue"),
    "b64ff7f3": dict(name="Martin Gierling",
        visit_type=None,
        procedures={"cryotherapy_single"},  # transcript shows applied (overrides clinician memory)
        procedures_optional=set(),
        forbidden_conditions={"chf_management","diabetic_assessment"},
        ok_dx={"707","216","459"},
        notes="Wart cryo applied per transcript; HTN management NOT CHF"),
}

# ── Variants ──────────────────────────────────────────────────────────────
V_BASE_PROMPT = open("/tmp/sim_billing/billing_current_prompt.txt").read()

# V_SOAP_ONLY: same prompt; user message provides SOAP only, no transcript
# V_DURATION: same prompt + explicit duration in user message
# V_STRICT: same prompt with extra rider that says "procedures from SOAP procedure[] only"
V_STRICT_RIDER = """

--- v0.10.61 SOAP→billing binding (SIMULATED) ---
The provided SOAP has a 5th "Procedure:" section listing procedures actually performed during the visit. The procedures array MUST be derived ONLY from that section. If the Procedure section is absent or empty, procedures MUST be []. Items in the Plan section that have NOT been moved to Procedure are FUTURE / DISCUSSED — do NOT bill them.
"""

V_PROC_BIND_PROMPT = V_BASE_PROMPT + """

--- v0.10.61 SIMULATED CODE-SIDE BINDING ---
The procedures array is overridden by the rule engine from the SOAP's procedure[] section AFTER your response. You may emit any procedures you think were performed; they will be DISCARDED by code. Focus your effort on visit_type, conditions, primaryDiagnosis, suggestedDiagnosticCode.
"""

VARIANTS = [
    ("V_BASE",      V_BASE_PROMPT,              "soap+transcript", False),
    ("V_SOAP_ONLY", V_BASE_PROMPT,              "soap_only",       False),
    ("V_DURATION",  V_BASE_PROMPT,              "soap+duration",   False),
    ("V_STRICT",    V_BASE_PROMPT + V_STRICT_RIDER, "soap+procsec", False),
    ("V_PROC_BIND", V_PROC_BIND_PROMPT,         "soap_only",       True),  # post-process: bind procs from SOAP
]

SEEDS = 2

# ── Plumbing ──────────────────────────────────────────────────────────────
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

def soap_to_text(soap_obj):
    """Render the v0.10.61 JSON SOAP as the bullet-point text format that
    matches what production stores on disk (so the billing prompt sees the
    same SOAP it would in production)."""
    parts = []
    for label, key in [("S:","subjective"),("O:","objective"),("A:","assessment"),("P:","plan")]:
        items = soap_obj.get(key) or []
        body = "\n".join(f"• {it}" for it in items if str(it).strip()) or "• Not documented"
        parts.append(f"{label}\n{body}")
    proc = soap_obj.get("procedure") or []
    if proc:
        body = "\n".join(
            (f"• {p.get('action','')}    [transcript: \"{p.get('transcript_quote','')}\"]"
             if p.get('transcript_quote') else f"• {p.get('action','')}")
            for p in proc if p.get("action","").strip()
        )
        if body:
            parts.append(f"Procedure:\n{body}")
    return "\n\n".join(parts)

def get_duration_minutes(sid):
    try:
        m = json.loads(base64.b64decode(open(f"/tmp/sim_billing/metadata_{sid}.b64").read()).decode())
        ms = m.get("duration_ms") or 0
        return int(ms / 60000) or 1
    except Exception:
        return 0

def build_user(sid, mode):
    soap_obj = json.load(open(f"/tmp/sim_billing/soap_v061_{sid}.json"))
    soap_text = soap_to_text(soap_obj)
    if mode == "soap_only":
        return f"## SOAP Note\n\n{soap_text}"
    if mode == "soap+duration":
        dur = get_duration_minutes(sid)
        return f"## SOAP Note\n\n{soap_text}\n\n## Visit Duration\n\n{dur} minutes (use to calibrate visit_type and time-tracking codes)"
    if mode == "soap+procsec":
        return f"## SOAP Note (5-section S/O/A/P/Procedure)\n\n{soap_text}"
    # default soap+transcript
    tr = base64.b64decode(open(f"/tmp/sim_billing/transcript_{sid}.b64").read()).decode()[-4500:]
    return f"## SOAP Note\n\n{soap_text}\n\n## Full Transcript\n\n{tr}"

def soap_procedure_bind(sid):
    """Deterministic procedure binding from SOAP.procedure[]."""
    soap_obj = json.load(open(f"/tmp/sim_billing/soap_v061_{sid}.json"))
    procs = soap_obj.get("procedure") or []
    out = []
    # Naive map from action text → procedure enum (could be improved)
    for p in procs:
        a = (p.get("action","") or "").lower()
        if "cryo" in a or "liquid nitrogen" in a or "frozen" in a:
            out.append("cryotherapy_single")
        elif "epidural" in a or "paravertebral" in a or "lumbar injection" in a or "spine" in a:
            out.append("nerve_block_paravertebral")
        elif "trigger point" in a:
            out.append("trigger_point_injection")
        elif "joint" in a or "knee inject" in a or "shoulder inject" in a or "cortisone" in a:
            out.append("joint_injection")
        elif "blood draw" in a or "venipuncture" in a:
            out.append("im_injection_with_visit")  # closest enum; venipuncture not directly listed
        elif "im injection" in a or "intramuscular" in a or "im/sc" in a:
            out.append("im_injection_with_visit")
    return out

def run_one(sid, vname, system, mode, post_bind, seed):
    user = build_user(sid, mode)
    raw = call(system, user)
    obj = extract_last_json_block(raw)
    if obj is None:
        return {"sid":sid,"variant":vname,"seed":seed,"err":True,"raw":raw[:200]}
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

def score_one(r, gt):
    s = {}
    # visit_type — pass if expected None or matches
    s["visit_ok"] = (gt.get("visit_type") is None) or (r.get("visit_type") == gt.get("visit_type"))
    # procedures — TP if non-forbidden / FP if hallucination outside acceptable + optional
    expected = set(gt["procedures"])
    optional = set(gt.get("procedures_optional", set()))
    actual = set(r["procedures"])
    s["proc_fp"] = len(actual - expected - optional)
    s["proc_fn"] = len(expected - actual)
    s["proc_ok"] = s["proc_fp"] == 0 and s["proc_fn"] == 0
    # conditions — fail if any forbidden condition appears
    forbidden = set(gt["forbidden_conditions"])
    bad = [c for c in r["conditions"] if c in forbidden]
    s["cond_hallucinated"] = bad
    s["cond_ok"] = len(bad) == 0
    # dx — pass if in ok_dx
    s["dx_ok"] = (r.get("dx") in gt["ok_dx"]) if gt["ok_dx"] else True
    s["dx_799_fallback"] = r.get("dx") == "799" and "799" not in gt["ok_dx"]
    s["all_ok"] = s["visit_ok"] and s["proc_ok"] and s["cond_ok"] and s["dx_ok"]
    return s

def main():
    results = []
    tasks = []
    for sid in GROUND_TRUTH:
        if not os.path.exists(f"/tmp/sim_billing/soap_v061_{sid}.json"):
            continue
        for vname, system, mode, post_bind in VARIANTS:
            for seed in range(SEEDS):
                tasks.append((sid, vname, system, mode, post_bind, seed))
    print(f"Total LLM calls: {len(tasks)}", flush=True)

    with ThreadPoolExecutor(max_workers=2) as ex:
        futs = {ex.submit(run_one, *t): t for t in tasks}
        for f in as_completed(futs):
            r = f.result()
            results.append(r)
            print(f"  {r['variant']:14} {r['sid']} seed={r['seed']}  visit={r.get('visit_type','?'):<22} dx={r.get('dx','?'):<5} procs={r.get('procedures',[])}", flush=True)

    # Score & aggregate per variant
    print("\n\n=== Scoring summary ===")
    by_variant = {}
    for r in results:
        if r.get("err"): continue
        gt = GROUND_TRUTH[r["sid"]]
        s = score_one(r, gt)
        r["score"] = s
        bv = by_variant.setdefault(r["variant"], {"all_ok":0,"visit_ok":0,"proc_ok":0,"cond_ok":0,"dx_ok":0,"dx_799":0,"n":0,"cond_hall":[]})
        bv["n"] += 1
        if s["all_ok"]: bv["all_ok"] += 1
        if s["visit_ok"]: bv["visit_ok"] += 1
        if s["proc_ok"]: bv["proc_ok"] += 1
        if s["cond_ok"]: bv["cond_ok"] += 1
        if s["dx_ok"]: bv["dx_ok"] += 1
        if s["dx_799_fallback"]: bv["dx_799"] += 1
        if s["cond_hallucinated"]: bv["cond_hall"].append((r["sid"], s["cond_hallucinated"]))

    print(f"\n{'variant':<14} {'all_ok':<8} {'visit':<8} {'proc':<8} {'cond':<8} {'dx':<8} {'dx799':<6}")
    for vname, bv in by_variant.items():
        n = bv["n"]
        def pct(x): return f"{x}/{n}"
        print(f"{vname:<14} {pct(bv['all_ok']):<8} {pct(bv['visit_ok']):<8} {pct(bv['proc_ok']):<8} {pct(bv['cond_ok']):<8} {pct(bv['dx_ok']):<8} {pct(bv['dx_799']):<6}")

    print("\n=== Condition hallucinations by variant ===")
    for vname, bv in by_variant.items():
        if bv["cond_hall"]:
            print(f"{vname}:")
            for sid, hall in bv["cond_hall"]:
                print(f"  {sid} ({GROUND_TRUTH[sid]['name']}) — {hall}")

    with open("/tmp/sim_billing/round1_results.json","w") as f:
        json.dump(results, f, indent=2)
    print("\nWritten /tmp/sim_billing/round1_results.json")

if __name__ == "__main__":
    main()
