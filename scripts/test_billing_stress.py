#!/usr/bin/env python3
"""
Stress test billing extraction with diverse, complex SOAP notes.

Tests edge cases, multi-procedure visits, ambiguous scenarios,
and complex clinical encounters to evaluate general robustness.

Usage:
    python3 scripts/test_billing_stress.py
"""

import json
import sys
import os
import requests
import time

# ═══════════════════════════════════════════════════════════════════
# CONFIG
# ═══════════════════════════════════════════════════════════════════
LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
MODEL = "fast-model"

def load_config():
    config_path = os.path.expanduser("~/.transcriptionapp/config.json")
    with open(config_path) as f:
        config = json.load(f)
    return config.get("llm_api_key", ""), config.get("llm_client_id", "ami-assist")

def load_system_prompt():
    with open("tauri-app/src-tauri/src/billing/clinical_features.rs") as f:
        content = f.read()
    start = content.find('let system_prompt = r#"') + len('let system_prompt = r#"')
    end = content.find('"#;', start)
    return content[start:end]

def call_llm(system_prompt, user_prompt):
    api_key, client_id = load_config()
    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "temperature": 0.1,
        "max_tokens": 500,
    }
    headers = {
        "Authorization": f"Bearer {api_key}",
        "X-Client-Id": client_id,
        "X-Clinic-Task": "billing_extraction",
        "Content-Type": "application/json",
    }
    try:
        resp = requests.post(LLM_URL, json=payload, headers=headers, timeout=120)
        resp.raise_for_status()
        return resp.json()["choices"][0]["message"]["content"]
    except Exception as e:
        return f"ERROR: {e}"

def parse_features(response):
    text = response.replace("```json", "").replace("```", "").strip()
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1:
        return None
    json_str = text[start:end+1]
    try:
        return json.loads(json_str)
    except:
        try:
            decoder = json.JSONDecoder()
            obj, _ = decoder.raw_decode(json_str)
            return obj
        except:
            return None

# Map procedure names to OHIP codes for validation
PROC_TO_CODE = {
    "joint_injection": "G370A",
    "joint_injection_additional": "G371A",
    "trigger_point_injection": "G384A",
    "trigger_point_additional": "G385A",
    "im_injection_with_visit": "G372A",
    "injection_sole_reason": "G373A",
    "nerve_block_peripheral": "G231A",
    "nerve_block_paravertebral": "G228A",
    "nail_excision_single": "Z128A",
    "ear_syringing": "G420A",
    "skin_biopsy": "Z113A",
    "biopsy_with_sutures": "Z116A",
    "cryotherapy_single": "Z117A",
    "cryotherapy_multiple": "Z117A",
    "cyst_excision_other": "Z125A",
    "cyst_excision_face": "Z122A",
    "pap_smear": "G365A",
    "iud_insertion": "G378A",
    "laceration_repair_simple_small": "Z154A",
    "immunization": "G538A",
    "immunization_flu": "G590A",
    "immunization_tdap": "G847A",
}

COND_TO_CODE = {
    "diabetes_management": "Q040A",
    "diabetic_assessment": "K030A",
    "smoking_cessation": "Q042A",
    "primary_mental_health": "K005A",
    "psychotherapy": "K007A",
    "sti_management": "K028A",
}

# ═══════════════════════════════════════════════════════════════════
# STRESS TEST CASES — complex, multi-procedure, ambiguous
# ═══════════════════════════════════════════════════════════════════
STRESS_TESTS = [
    {
        "name": "S1. Bilateral knee injections (2 joints)",
        "soap": """S: Bilateral knee OA. Both knees painful, left worse than right.
O: Left knee: moderate effusion, crepitus. Right knee: mild crepitus, no effusion. Both with reduced ROM.
A: Bilateral knee osteoarthritis.
P: Cortisone injection performed in left knee and right knee. Follow-up in 8 weeks.""",
        "must_include_procs": ["joint_injection", "joint_injection_additional"],
        "must_not_include_procs": ["injection_sole_reason"],
        "notes": "Two joints = G370A + G371A. NOT two G370A codes."
    },
    {
        "name": "S2. Joint injection + trigger point in same visit",
        "soap": """S: Right shoulder pain and upper back tension. Shoulder has been painful for weeks, difficulty reaching overhead.
O: Right glenohumeral joint tender, reduced abduction. Trigger point in right upper trapezius.
A: Right shoulder impingement + myofascial trigger point.
P: Cortisone injection into right shoulder joint. Trigger point injection at right upper trapezius with lidocaine.""",
        "must_include_procs": ["joint_injection", "trigger_point_injection"],
        "must_not_include_procs": ["injection_sole_reason"],
        "notes": "Two different procedure types in same visit."
    },
    {
        "name": "S3. Laceration repair + tetanus shot",
        "soap": """S: Cut on right hand from kitchen knife 2 hours ago. Last tetanus shot >10 years ago.
O: 3cm clean laceration on dorsal right hand, full thickness skin. Tendons intact. No foreign body.
A: Laceration right hand, clean. Tetanus booster due.
P: Wound irrigated. Laceration repaired with 4 interrupted 4-0 nylon sutures. Tdap vaccine administered. Return for suture removal in 10 days.""",
        "must_include_procs": ["laceration_repair_simple_small", "immunization_tdap"],
        "must_not_include_procs": [],
        "notes": "Laceration (Z154A) + Tdap (G847A)."
    },
    {
        "name": "S4. Pap smear + IUD insertion in same visit",
        "soap": """S: Here for IUD insertion. Also due for Pap smear.
O: Normal pelvic exam. Pap smear collected. Mirena IUD inserted without complications.
A: Contraception counselling. Cervical screening performed.
P: Pap specimen sent. IUD in situ, strings trimmed. Follow-up 6 weeks for string check.""",
        "must_include_procs": ["pap_smear", "iud_insertion"],
        "must_not_include_procs": [],
        "notes": "Two gynecology procedures in one visit."
    },
    {
        "name": "S5. Multiple skin procedures — cryo + biopsy",
        "soap": """S: Several skin concerns. Suspicious mole on back, plus actinic keratoses on face.
O: 6mm irregular pigmented lesion on upper back. Three actinic keratoses on forehead.
A: Suspicious melanocytic lesion back. Actinic keratoses forehead.
P: Excisional biopsy of back lesion with sutures — sent to pathology. Cryotherapy applied to all three forehead lesions.""",
        "must_include_procs": ["biopsy_with_sutures", "cryotherapy_multiple"],
        "must_not_include_procs": [],
        "notes": "Biopsy (Z116A) + cryo (Z117A) in same visit."
    },
    {
        "name": "S6. Flu shot only — walk-in, no assessment",
        "soap": """S: Here for flu vaccine only. No other concerns today.
O: Influenza vaccine administered in left deltoid.
A: Influenza immunization.
P: Observe 15 minutes post-injection.""",
        "must_include_procs": ["immunization_flu"],
        "must_not_include_procs": ["joint_injection", "trigger_point_injection"],
        "notes": "Sole reason injection. Visit type should be minimal or injection_sole_reason."
    },
    {
        "name": "S7. Diabetes + smoking cessation in same visit",
        "soap": """S: Diabetes follow-up. A1C 7.8%. Also wants to discuss quitting smoking — smokes 15/day.
O: BP 130/80, BMI 29. Foot exam normal. Discussed NRT options.
A: Type 2 DM, suboptimal control. Tobacco use disorder.
P: Increase metformin. Prescribed Champix for smoking cessation. Set quit date. Recheck A1C and follow-up for cessation in 6 weeks.""",
        "must_include_procs": [],
        "must_include_conds": ["diabetic_assessment", "smoking_cessation"],
        "must_not_include_procs": [],
        "notes": "Two conditions managed: K030A + Q042A."
    },
    {
        "name": "S8. Phone call diabetes check — virtual care",
        "soap": """S: Phone call follow-up for diabetes. Patient reports doing well on new medication. No hypoglycemia episodes.
O: (phone assessment — no physical exam). Reviewed recent lab results: A1C 7.0%, down from 7.8%.
A: Type 2 DM, improved control on current regimen.
P: Continue current medications. Recheck A1C in 3 months.""",
        "must_include_procs": [],
        "expected_setting": "telephone_in_office",
        "must_include_conds": ["diabetic_assessment"],
        "must_not_include_procs": [],
        "notes": "Virtual phone visit. Should be A102A or virtual_phone, not in-office."
    },
    {
        "name": "S9. Complex — nerve block + joint injection + trigger point",
        "soap": """S: Chronic pain right shoulder and neck. Failed conservative management.
O: Reduced ROM right shoulder, impingement signs positive. Trigger point at right levator scapulae. Cervical paravertebral tenderness.
A: Right shoulder impingement. Cervical myofascial pain. Cervical radiculopathy.
P: Cortisone injection right shoulder joint. Trigger point injection right levator scapulae. Paravertebral nerve block C5-C6 with bupivacaine.""",
        "must_include_procs": ["joint_injection", "trigger_point_injection", "nerve_block_paravertebral"],
        "must_not_include_procs": ["injection_sole_reason"],
        "notes": "THREE different procedure types. All should be extracted."
    },
    {
        "name": "S10. Senior annual physical with multiple vaccines",
        "soap": """S: Annual physical, 70-year-old female. Due for flu shot and pneumococcal vaccine.
O: BP 128/76, BMI 22. Full exam unremarkable. Cognitive screen MMSE 29/30.
A: Healthy senior. Age-appropriate screening up to date.
P: Influenza and pneumococcal conjugate vaccines administered. Screening bloodwork ordered. Colorectal screening discussed.""",
        "must_include_procs": ["immunization_flu", "immunization_pneumococcal"],
        "expected_visit_options": ["periodic_health_senior"],
        "must_not_include_procs": [],
        "notes": "K132A + G590A + G846A."
    },
    {
        "name": "S11. Mental health — psychotherapy session",
        "soap": """S: Follow-up for depression and anxiety. On sertraline 100mg. Reports improved sleep but ongoing rumination.
O: Euthymic affect but anxious. PHQ-9 score 8 (mild). GAD-7 score 12 (moderate).
A: Major depressive disorder, improving. Generalized anxiety disorder, moderate.
P: CBT techniques reviewed. Continue sertraline. Mindfulness exercises discussed. Follow-up 4 weeks.""",
        "must_include_procs": [],
        "must_include_conds": ["psychotherapy"],
        "must_not_include_procs": [],
        "notes": "K007A psychotherapy. NOT K005A (primary mental health) — this is an active therapy session."
    },
    {
        "name": "S12. Abscess drainage + IM antibiotic",
        "soap": """S: Painful red lump right buttock x 3 days, getting bigger. No fever.
O: 4cm fluctuant abscess right gluteal area. Erythema surrounding. No cellulitis spreading.
A: Gluteal abscess.
P: Incision and drainage performed. Wound packed. IM ceftriaxone 250mg administered. Wound care instructions. Return for packing change in 2 days.""",
        "must_include_procs": ["abscess_drainage", "im_injection_with_visit"],
        "must_not_include_procs": ["injection_sole_reason"],
        "notes": "Z101A (abscess I&D) + G372A (IM injection with visit). NOT G373A."
    },
    {
        "name": "S13. Ear syringing + skin tag removal",
        "soap": """S: Ears blocked, can't hear. Also wants skin tags on neck removed.
O: Bilateral cerumen impaction — syringing performed, TMs normal after. 4 small skin tags on neck.
A: Cerumen impaction resolved. Multiple acrochordons.
P: Ears clear. Skin tags removed by electrocoagulation (4 lesions). Follow-up PRN.""",
        "must_include_procs": ["ear_syringing", "electrocoagulation_multiple"],
        "must_not_include_procs": [],
        "notes": "G420A + Z160A (or Z161A for 3+). Two unrelated procedures."
    },
    {
        "name": "S14. House call for elderly patient",
        "soap": """S: Home visit for 88-year-old housebound patient. Daughter called — father confused today.
O: In bed, oriented x1. BP 150/90, HR 88 irregular. No focal neuro deficit. Lungs crackles bilateral bases.
A: Delirium — r/o UTI, CHF exacerbation. Atrial fibrillation.
P: Urine dip positive for nitrites. Started Macrobid. Increased furosemide. Blood work ordered. Reassess in 2 days.""",
        "must_include_procs": [],
        "expected_visit_options": ["house_call"],
        "must_not_include_procs": [],
        "notes": "A900A house call assessment."
    },
    {
        "name": "S15. Foreign body removal + laceration repair",
        "soap": """S: Stepped on glass at the beach. Piece of glass still in right foot.
O: 1cm laceration plantar right foot. Glass fragment visible. Removed with forceps under local anaesthesia. Wound irrigated.
A: Foreign body right foot, removed. Laceration.
P: Foreign body removed. Laceration sutured with 3-0 nylon. Tetanus status up to date. Return in 7 days for suture removal.""",
        "must_include_procs": ["foreign_body_removal", "laceration_repair_simple_small"],
        "must_not_include_procs": [],
        "notes": "Z114A (foreign body removal) + Z154A (laceration repair). Both performed."
    },
]


def main():
    system_prompt = load_system_prompt()

    print("=" * 70)
    print("BILLING EXTRACTION STRESS TEST")
    print(f"Model: {MODEL}")
    print(f"Test cases: {len(STRESS_TESTS)}")
    print("=" * 70)

    results = []
    pass_count = 0
    fail_count = 0

    for tc in STRESS_TESTS:
        print(f"\n{'─' * 70}")
        print(f"{tc['name']}")
        print(f"{'─' * 70}")

        user_prompt = f"## SOAP Note\n\n{tc['soap']}\n\n## Full Transcript\n\n(transcript not available for this test)"

        start = time.time()
        response = call_llm(system_prompt, user_prompt)
        latency = time.time() - start

        if response.startswith("ERROR:"):
            print(f"  LLM ERROR: {response}")
            results.append({"name": tc["name"], "pass": False, "error": response})
            fail_count += 1
            continue

        features = parse_features(response)
        if features is None:
            print(f"  PARSE ERROR: {response[:200]}")
            results.append({"name": tc["name"], "pass": False, "error": "parse_failed"})
            fail_count += 1
            continue

        visit = features.get("visitType", "")
        procs = set(features.get("procedures", []))
        conds = set(features.get("conditions", []))
        setting = features.get("setting", "")

        # Check constraints
        issues = []

        # Must-include procedures
        must_procs = set(tc.get("must_include_procs", []))
        missing_procs = must_procs - procs
        if missing_procs:
            issues.append(f"MISSING procedures: {missing_procs}")

        # Must-not-include procedures
        must_not = set(tc.get("must_not_include_procs", []))
        bad_procs = must_not & procs
        if bad_procs:
            issues.append(f"WRONG procedures (should not be present): {bad_procs}")

        # Must-include conditions
        must_conds = set(tc.get("must_include_conds", []))
        missing_conds = must_conds - conds
        if missing_conds:
            issues.append(f"MISSING conditions: {missing_conds}")

        # Expected visit options
        visit_opts = tc.get("expected_visit_options", [])
        if visit_opts and visit not in visit_opts:
            issues.append(f"VISIT: got {visit}, expected one of {visit_opts}")

        # Expected setting
        exp_setting = tc.get("expected_setting")
        if exp_setting and setting != exp_setting:
            issues.append(f"SETTING: got {setting}, expected {exp_setting}")

        passed = len(issues) == 0
        status = "PASS" if passed else "FAIL"
        if passed:
            pass_count += 1
        else:
            fail_count += 1

        # Print results
        print(f"  Visit:      {visit}")
        print(f"  Procedures: {sorted(procs) if procs else '(none)'}")
        print(f"  Conditions: {sorted(conds) if conds else '(none)'}")
        if setting != "in_office":
            print(f"  Setting:    {setting}")
        print(f"  Latency:    {latency:.1f}s")
        print(f"  Result:     {status}")
        if issues:
            for issue in issues:
                print(f"  *** {issue}")
        if tc.get("notes"):
            print(f"  Notes:      {tc['notes']}")

        # Map to OHIP codes for reference
        ohip_codes = []
        for p in sorted(procs):
            code = PROC_TO_CODE.get(p, f"?{p}")
            ohip_codes.append(code)
        for c in sorted(conds):
            code = COND_TO_CODE.get(c, f"?{c}")
            ohip_codes.append(code)
        if ohip_codes:
            print(f"  OHIP codes: {', '.join(ohip_codes)}")

        results.append({
            "name": tc["name"],
            "pass": passed,
            "visit": visit,
            "procs": sorted(procs),
            "conds": sorted(conds),
            "issues": issues,
            "latency": latency,
        })

    # Summary
    print(f"\n{'=' * 70}")
    print("STRESS TEST SUMMARY")
    print(f"{'=' * 70}")
    print(f"  Passed: {pass_count}/{len(STRESS_TESTS)} ({100*pass_count//len(STRESS_TESTS)}%)")
    print(f"  Failed: {fail_count}/{len(STRESS_TESTS)}")

    if fail_count > 0:
        print(f"\n  FAILURES:")
        for r in results:
            if not r["pass"] and "issues" in r:
                print(f"    - {r['name']}")
                for issue in r["issues"]:
                    print(f"      {issue}")

    with open("/tmp/billing_stress_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print(f"\n  Results written to /tmp/billing_stress_results.json")

    sys.exit(0 if fail_count == 0 else 1)

if __name__ == "__main__":
    main()
