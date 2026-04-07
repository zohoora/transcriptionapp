#!/usr/bin/env python3
"""
Test billing extraction against a variety of SOAP notes.

Sends each test SOAP note through the LLM billing extraction prompt,
parses the response, and checks if the expected codes are produced.

Usage:
    python3 scripts/test_billing_extraction.py

Requires: LLM Router running at the configured URL.
"""

import json
import sys
import requests
import time

# ═══════════════════════════════════════════════════════════════════
# CONFIG
# ═══════════════════════════════════════════════════════════════════
LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
MODEL = "fast-model"

# ═══════════════════════════════════════════════════════════════════
# TEST CASES
# Each has: name, soap_note, transcript_excerpt, expected_codes
# ═══════════════════════════════════════════════════════════════════
TEST_CASES = [
    {
        "name": "1. Simple follow-up — knee OA with cortisone injection",
        "soap": """S: Follow-up for right knee osteoarthritis. Pain 6/10, worse going up stairs. Using ibuprofen PRN.
O: Right knee: moderate effusion, crepitus on flexion, ROM 0-110 degrees. No instability.
A: Right knee osteoarthritis, moderate. Effusion.
P: Cortisone injection into right knee joint performed today. Continue physiotherapy. Follow-up 6 weeks.""",
        "transcript": "Patient here for knee follow-up. I'm going to inject your knee with cortisone today.",
        "expected_visit": "intermediate_assessment",
        "expected_procedures": ["joint_injection"],
        "expected_conditions": [],
        "notes": "Should pick joint_injection (G370A), NOT injection_sole_reason (G373A)"
    },
    {
        "name": "2. Multiple trigger point injections — chronic back pain",
        "soap": """S: Chronic myofascial pain in neck and upper back. Multiple tender spots. Pain 7/10.
O: Trigger points palpated at bilateral upper trapezius, right levator scapulae, and left rhomboid. 4 sites total.
A: Myofascial pain syndrome with multiple trigger points.
P: Trigger point injections performed at 4 sites with 1% lidocaine. Ice and stretching advised.""",
        "transcript": "I can feel the trigger points in your traps and your levator. I'm going to inject four spots today.",
        "expected_visit": "intermediate_assessment",
        "expected_procedures": ["trigger_point_injection", "trigger_point_additional"],
        "expected_conditions": [],
        "notes": "Should pick G384A + G385A, NOT G373A"
    },
    {
        "name": "3. Well-baby visit with immunizations",
        "soap": """S: 4-month well-baby visit. Mother reports feeding well, sleeping through the night. No concerns.
O: Weight 6.2kg (50th percentile), length 62cm (50th). Alert, social smile. Anterior fontanelle soft, flat. Heart and lungs normal. Hips stable.
A: Healthy 4-month-old, appropriate growth and development.
P: DTaP-IPV-Hib and pneumococcal conjugate vaccines administered. Next visit at 6 months.""",
        "transcript": "Baby is doing great. We'll give the four month shots today.",
        "expected_visit": "well_baby_visit",
        "expected_procedures": ["immunization_pediatric", "immunization_pneumococcal"],
        "expected_conditions": [],
        "notes": "Should pick specific vaccine codes G840A/G841A + G846A"
    },
    {
        "name": "4. Diabetes management with A1C review",
        "soap": """S: Diabetes follow-up. A1C came back at 8.1%, up from 7.4%. Taking metformin 1000mg BID. Reports poor diet compliance over holidays.
O: BP 138/82, BMI 32. Feet: sensation intact, no ulcers, pedal pulses present.
A: Type 2 diabetes, suboptimal control. A1C worsening.
P: Increase metformin to 1500mg BID. Dietary counselling provided. Referral to diabetes education program. Recheck A1C in 3 months.""",
        "transcript": "Your sugar has gone up. Let me check your feet. I want to increase your metformin.",
        "expected_visit": ["general_reassessment", "intermediate_assessment"],
        "expected_procedures": [],
        "expected_conditions": ["diabetic_assessment", "diabetes_management"],
        "notes": "Should pick K030A + Q040A. Visit type debatable (A004A or A007A)."
    },
    {
        "name": "5. Ear syringing — bilateral cerumen impaction",
        "soap": """S: Both ears feel blocked. Can't hear well for past 2 weeks.
O: Bilateral cerumen impaction. After syringing: TMs visualized bilaterally, normal appearance.
A: Bilateral cerumen impaction, resolved with syringing.
P: Return if symptoms recur. Advised against using cotton swabs.""",
        "transcript": "Your ears are packed with wax. I'll flush them out for you.",
        "expected_visit": "minor_assessment",
        "expected_procedures": ["ear_syringing"],
        "expected_conditions": [],
        "notes": "Should pick G420A for ear syringing"
    },
    {
        "name": "6. Skin biopsy with sutures — suspicious mole",
        "soap": """S: Concerned about changing mole on left shoulder. Getting darker and larger over 3 months.
O: 8mm irregularly pigmented macule on left posterior shoulder. Asymmetric, irregular borders, color variation.
A: Suspicious melanocytic lesion, requires biopsy.
P: Excisional biopsy performed with 4-0 nylon sutures. Specimen sent to pathology. Follow-up for results in 2 weeks.""",
        "transcript": "This mole looks concerning. I'm going to cut it out and send it to the lab. I'll close it with sutures.",
        "expected_visit": "intermediate_assessment",
        "expected_procedures": ["biopsy_with_sutures"],
        "expected_conditions": [],
        "notes": "Should pick Z116A (biopsy WITH sutures), NOT Z113A (without)"
    },
    {
        "name": "7. Cryotherapy — multiple warts",
        "soap": """S: Multiple warts on both hands. Had cryotherapy before, they came back.
O: 6 verruca vulgaris lesions across dorsal surfaces of both hands.
A: Multiple common warts, recurrent.
P: Cryotherapy with liquid nitrogen applied to all 6 lesions. May need repeat treatment in 3 weeks.""",
        "transcript": "I'll freeze all of these warts today.",
        "expected_visit": ["minor_assessment", "intermediate_assessment"],
        "expected_procedures": ["cryotherapy_multiple"],
        "expected_conditions": [],
        "notes": "Should pick Z117A. Visit type debatable (minor or intermediate for 6 lesions)."
    },
    {
        "name": "8. Annual physical — adult 45 years old",
        "soap": """S: Here for annual physical. No active complaints. Non-smoker. Exercises 3x/week.
O: BP 122/78, BMI 24.5. Full physical exam unremarkable.
A: Healthy 45-year-old male. Due for colorectal screening discussion.
P: Requisition for fasting lipids, fasting glucose, CBC. Discussed colorectal screening options. FOBT kit provided. Due for tetanus booster — Tdap administered.""",
        "transcript": "This is your annual checkup. Everything looks good. Let's do your tetanus booster while you're here.",
        "expected_visit": "periodic_health_adult",
        "expected_procedures": ["immunization_tdap"],
        "expected_conditions": [],
        "notes": "Should pick K131A (periodic health 18-64), NOT A003A. Plus G847A for Tdap."
    },
    {
        "name": "9. Smoking cessation counselling — dedicated session",
        "soap": """S: Wants to quit smoking. Smoking 1 pack/day for 20 years. Previous failed attempt with patch.
O: Appears motivated. Fagerstrom score 7 (high dependence).
A: Tobacco use disorder, high nicotine dependence. Ready to quit.
P: Discussed NRT options, varenicline. Set quit date for 2 weeks. Prescribed Champix. Follow-up in 4 weeks.""",
        "transcript": "Let's talk about quitting smoking today. This is a dedicated counselling visit.",
        "expected_visit": "counselling",
        "expected_procedures": [],
        "expected_conditions": ["smoking_cessation"],
        "notes": "Should pick K013A (counselling) + Q042A (smoking cessation fee)"
    },
    {
        "name": "10. Ingrown toenail removal",
        "soap": """S: Painful ingrown toenail right great toe for 2 weeks. Getting worse.
O: Right hallux: medial nail border embedded in lateral nail fold. Erythema, mild purulence.
A: Ingrown toenail right great toe with paronychia.
P: Digital nerve block performed. Partial nail avulsion of medial border with phenol matrixectomy. Wound care instructions given.""",
        "transcript": "I'll freeze your toe and remove the ingrown part of the nail.",
        "expected_visit": ["minor_assessment", "intermediate_assessment"],
        "expected_procedures": ["nail_excision_single"],
        "expected_conditions": [],
        "notes": "Should pick Z128A. Nerve block (G231A) also acceptable as additional procedure."
    },
    {
        "name": "11. Lipoma excision — back",
        "soap": """S: Lump on upper back for 1 year. Slowly growing. No pain.
O: 3cm soft, mobile, non-tender subcutaneous mass on right upper back. Consistent with lipoma.
A: Lipoma, right upper back.
P: Excision performed under local anaesthesia. Specimen sent to pathology. Wound closed with interrupted sutures. Follow-up for suture removal in 10 days.""",
        "transcript": "That's a lipoma. I'll cut it out today under local freezing.",
        "expected_visit": "intermediate_assessment",
        "expected_procedures": ["cyst_excision_other"],
        "expected_conditions": [],
        "notes": "Should pick Z125A (Group 3 cyst/lipoma other areas)"
    },
    {
        "name": "12. Paravertebral nerve block — lower back",
        "soap": """S: Acute low back pain radiating to left leg. Started 3 days ago after lifting. Pain 8/10.
O: Lumbar paravertebral muscle spasm. SLR positive left at 40 degrees. No motor deficit.
A: Acute lumbar radiculopathy with paravertebral muscle spasm.
P: Paravertebral nerve block performed at L4-L5 level with bupivacaine and methylprednisolone. Significant pain relief noted. Refer for physiotherapy.""",
        "transcript": "I'm going to do a nerve block in your lower back to help with the pain.",
        "expected_visit": "intermediate_assessment",
        "expected_procedures": ["nerve_block_paravertebral"],
        "expected_conditions": [],
        "notes": "Should pick G228A (paravertebral), NOT G231A (peripheral)"
    },
]


# ═══════════════════════════════════════════════════════════════════
# LOAD THE PROMPT FROM THE RUST SOURCE
# ═══════════════════════════════════════════════════════════════════
def load_system_prompt():
    """Extract the system prompt from clinical_features.rs"""
    with open("tauri-app/src-tauri/src/billing/clinical_features.rs") as f:
        content = f.read()

    # Find the prompt between r#" and "#
    start = content.find('let system_prompt = r#"')
    if start == -1:
        print("ERROR: Could not find system prompt in clinical_features.rs")
        sys.exit(1)
    start += len('let system_prompt = r#"')
    end = content.find('"#;', start)
    return content[start:end]


def load_config():
    """Load API key and client ID from app config"""
    import os
    config_path = os.path.expanduser("~/.transcriptionapp/config.json")
    with open(config_path) as f:
        config = json.load(f)
    return config.get("llm_api_key", ""), config.get("llm_client_id", "ami-assist")


def call_llm(system_prompt, user_prompt):
    """Call the LLM router"""
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
    """Parse the LLM response into a dict"""
    import re
    # Strip code fences
    text = response.replace("```json", "").replace("```", "").strip()
    # Find JSON
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1:
        return None
    json_str = text[start:end+1]
    try:
        return json.loads(json_str)
    except:
        # Try streaming parse (handle trailing content)
        try:
            decoder = json.JSONDecoder()
            obj, _ = decoder.raw_decode(json_str)
            return obj
        except:
            return None


def main():
    system_prompt = load_system_prompt()

    print("=" * 70)
    print("BILLING EXTRACTION TEST SUITE")
    print(f"Model: {MODEL}")
    print(f"Test cases: {len(TEST_CASES)}")
    print("=" * 70)

    results = []

    for i, tc in enumerate(TEST_CASES):
        print(f"\n{'─' * 70}")
        print(f"TEST {tc['name']}")
        print(f"{'─' * 70}")

        user_prompt = f"## SOAP Note\n\n{tc['soap']}\n\n## Full Transcript\n\n{tc['transcript']}"

        start = time.time()
        response = call_llm(system_prompt, user_prompt)
        latency = time.time() - start

        if response.startswith("ERROR:"):
            print(f"  LLM ERROR: {response}")
            results.append({"name": tc["name"], "pass": False, "error": response})
            continue

        features = parse_features(response)
        if features is None:
            print(f"  PARSE ERROR: {response[:200]}")
            results.append({"name": tc["name"], "pass": False, "error": "parse_failed"})
            continue

        # Check results
        visit = features.get("visitType", "")
        procs = features.get("procedures", [])
        conds = features.get("conditions", [])

        expected_visits = tc["expected_visit"] if isinstance(tc["expected_visit"], list) else [tc["expected_visit"]]
        visit_ok = visit in expected_visits
        # Allow extra procedures (e.g., nerve block with nail excision is fine)
        procs_ok = set(tc["expected_procedures"]).issubset(set(procs))
        conds_ok = set(tc["expected_conditions"]).issubset(set(conds))

        all_ok = visit_ok and procs_ok and conds_ok
        status = "PASS" if all_ok else "FAIL"

        expected_visit_str = str(tc['expected_visit']) if isinstance(tc['expected_visit'], list) else tc['expected_visit']
        print(f"  Visit:      {visit:30s} {'✓' if visit_ok else '✗ expected: ' + expected_visit_str}")
        print(f"  Procedures: {str(procs):50s}")
        if not procs_ok:
            print(f"              ✗ expected: {tc['expected_procedures']}")
        else:
            print(f"              ✓")
        print(f"  Conditions: {str(conds):50s}")
        if not conds_ok:
            print(f"              ✗ expected: {tc['expected_conditions']}")
        else:
            print(f"              ✓")
        print(f"  Latency:    {latency:.1f}s")
        print(f"  Result:     {status}")
        if tc.get("notes"):
            print(f"  Notes:      {tc['notes']}")

        results.append({
            "name": tc["name"],
            "pass": all_ok,
            "visit": visit,
            "visit_expected": tc["expected_visit"],
            "procs": procs,
            "procs_expected": tc["expected_procedures"],
            "conds": conds,
            "conds_expected": tc["expected_conditions"],
            "latency": latency,
        })

    # Summary
    print(f"\n{'=' * 70}")
    print("SUMMARY")
    print(f"{'=' * 70}")
    passed = sum(1 for r in results if r["pass"])
    total = len(results)
    print(f"  Passed: {passed}/{total}")

    if passed < total:
        print(f"\n  FAILURES:")
        for r in results:
            if not r["pass"]:
                print(f"    - {r['name']}")
                if r.get("visit") != r.get("visit_expected"):
                    print(f"      Visit: got {r.get('visit')}, expected {r.get('visit_expected')}")
                if set(r.get("procs", [])) != set(r.get("procs_expected", [])):
                    print(f"      Procs: got {r.get('procs')}, expected {r.get('procs_expected')}")
                if not set(r.get("conds", [])) >= set(r.get("conds_expected", [])):
                    print(f"      Conds: got {r.get('conds')}, expected {r.get('conds_expected')}")

    # Write JSON results
    with open("/tmp/billing_test_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print(f"\n  Results written to /tmp/billing_test_results.json")

    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
