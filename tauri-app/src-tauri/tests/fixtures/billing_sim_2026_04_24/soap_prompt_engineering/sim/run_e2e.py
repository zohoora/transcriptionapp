#!/usr/bin/env python3
"""End-to-end weekly simulation: regenerate SOAPs through a prompt that
requires a Procedure section with transcript citations, then run billing
extraction (post-fix prompt) on those new SOAPs.

Compares to V1 baseline = the SOAP + billing actually stored on disk for
each of this week's procedure-billing sessions.
"""

import json, os, sys, urllib.request, urllib.error, base64, re
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL  = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY  = "ai-scribe-secret-key"
SOAP_MODEL = "soap-model-fast"
BILL_MODEL = "fast-model"
CLIENT   = "ai-scribe"

SESSIONS = [
    ("0cce8568", "Catherine Lamoureux", "Apr 20"),
    ("70e369bf", "(unknown — Apr 20)",  "Apr 20"),
    ("58e617ac", "Linda Routledge",     "Apr 21"),
    ("d8c25a19", "(unknown — Apr 22)",  "Apr 22"),
    ("e11d3112", "Catherine Grieve",    "Apr 22"),
    ("173c4253", "Helene Williamson",   "Apr 23"),
    ("1d853542", "Julie Hundert",       "Apr 23"),
    ("8feb77a1", "Hasan Mirza",         "Apr 23"),
    ("9b0b60e2", "Mary Beth Dop",       "Apr 23"),
    ("1ec7a57e", "Alexander Gulas",     "Apr 24"),
    ("475695fe", "Shirley Muir",        "Apr 24"),
    ("98821f67", "Louise Simon",        "Apr 24"),
    ("b64ff7f3", "Martin Gierling",     "Apr 24"),
]

BILL_V2_PROMPT = open("/tmp/sim_e2e/billing_prompt_v2.txt").read()
BILL_V2_PROMPT += """

--- ADDITIONAL PROCEDURE-SECTION RULE (v0.10.60 simulation) ---
The SOAP note now contains an explicit "Procedure:" section listing procedures actually performed during this visit.
- The procedures array MUST be derived ONLY from the Procedure section.
- If the Procedure section is absent or marked "(none)", procedures MUST be an empty array.
- Items in the Plan section that have NOT been moved to the Procedure section are DISCUSSED / FUTURE / OFFERED — they are NOT billable.
"""

# New SOAP prompt — adds Procedure section with mandatory transcript citation per line.
SOAP_PROCSEC_PROMPT = """You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from the transcript into a 5-section SOAP-with-Procedure note.

The transcript is from speech-to-text and may contain errors. Interpret medical terms correctly:
- "human blade 1c" or "h b a 1 c" → HbA1c
- "ekg" → EKG
- Homophones and phonetic errors are common — use clinical context.

RESPOND WITH ONLY THIS JSON STRUCTURE — NO OTHER TEXT:
{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"],"procedure":[{"action":"<past-tense procedure performed>","transcript_quote":"<verbatim doctor quote from transcript proving it was DONE, not discussed>"}]}

SECTION DEFINITIONS:
- SUBJECTIVE: What the patient reports — symptoms, complaints, history of present illness, past medical/surgical history, medication history, social history, family history, review of systems.
- OBJECTIVE: ONLY findings from TODAY'S encounter — vital signs measured today, physical exam findings observed by the clinician, point-of-care test results obtained today.
- ASSESSMENT: Clinical impressions, diagnoses, differential diagnoses.
- PLAN: Treatments ordered, prescriptions, referrals, follow-up instructions, patient education, next steps. Procedures the clinician PROPOSED, OFFERED, or SCHEDULED for the future belong here, NOT in Procedure.
- PROCEDURE: Procedures the clinician PHYSICALLY PERFORMED during this visit. Each entry MUST include:
  * "action" — past-tense statement of what was done (e.g. "Liquid nitrogen applied to plantar wart", "Cortisone injection administered into right knee", "Skin biopsy taken from left forearm").
  * "transcript_quote" — a VERBATIM quote of the doctor's own past-tense speech from the transcript proving the procedure happened (e.g. "I sprayed it", "I just injected the knee", "I drew the blood").
  If you cannot find a verbatim past-tense doctor quote in the transcript proving the procedure was performed, DO NOT add it to the procedure array. Empty array [] is correct when no procedure was performed.
  Modal verbs ("could", "would", "might", "we'll"), offers ("I could spray it", "let me grab the nitrogen"), and scheduling ("inject next time", "arrange B12 injection") are NOT performed procedures.

Rules:
- Output VALID JSON only. No prose, no markdown.
- Use empty arrays [] for sections with no information.
- Do NOT include patient names; use "patient" or "the physician".
- Do NOT hallucinate — only include what was explicitly stated.
- Be concise (point-form / phrase fragments preferred for S/O/A/P).

DETAIL LEVEL: 5 (moderate detail). FORMAT: comprehensive — single integrated note covering all topics discussed."""

def call_llm(model: str, system: str, user: str, task: str, timeout=120) -> str:
    payload = json.dumps({
        "model": model,
        "messages": [{"role":"system","content":system},{"role":"user","content":user}],
        "temperature": 0.1,
    }).encode()
    req = urllib.request.Request(LLM_URL, data=payload, headers={
        "Content-Type":"application/json",
        "Authorization": f"Bearer {LLM_KEY}",
        "X-Client-Id": CLIENT,
        "X-Clinic-Task": task,
    })
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            d = json.loads(r.read())
            return d["choices"][0]["message"]["content"]
    except urllib.error.HTTPError as e:
        return f"<HTTP {e.code}: {e.read()[:200].decode(errors='replace')}>"
    except Exception as e:
        return f"<ERR {e}>"

def extract_last_json_block(text: str):
    if text.startswith("<"): return None
    # Strip Markdown fences
    text = re.sub(r"```(?:json)?", "", text).replace("```","")
    blocks = []; depth = 0; start = None; in_str = False; esc = False
    for i, c in enumerate(text):
        if in_str:
            if esc: esc = False
            elif c == "\\": esc = True
            elif c == '"': in_str = False
            continue
        if c == '"': in_str = True
        elif c == "{":
            if depth == 0: start = i
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0 and start is not None:
                blocks.append(text[start:i+1]); start = None
    for b in reversed(blocks):
        try: return json.loads(b)
        except Exception: continue
    return None

def soap_json_to_text(soap_obj: dict) -> str:
    """Render a regenerated SOAP-JSON into the text form the billing extractor expects."""
    def section(label, items):
        if not items: return f"{label}\n"
        if isinstance(items, list):
            lines = []
            for it in items:
                if isinstance(it, dict):
                    a = it.get("action","")
                    q = it.get("transcript_quote","")
                    lines.append(f"• {a}    [transcript: \"{q}\"]" if q else f"• {a}")
                else:
                    lines.append(f"• {it}")
            return f"{label}\n" + "\n".join(lines) + "\n"
        return f"{label}\n• {items}\n"
    out = ""
    out += section("S:", soap_obj.get("subjective", []))
    out += "\n" + section("O:", soap_obj.get("objective", []))
    out += "\n" + section("A:", soap_obj.get("assessment", []))
    out += "\n" + section("P:", soap_obj.get("plan", []))
    out += "\n" + section("Procedure:", soap_obj.get("procedure", []))
    return out

def process_session(sid, name, date):
    transcript = base64.b64decode(open(f"/tmp/sim_e2e/tr_{sid}.b64").read()).decode()
    baseline_soap = base64.b64decode(open(f"/tmp/sim_e2e/soap_{sid}.b64").read()).decode()
    baseline_billing = json.loads(base64.b64decode(open(f"/tmp/sim_e2e/bill_{sid}.b64").read()).decode())
    v1_procs = [c["code"] for c in baseline_billing.get("codes", []) if c["code"].startswith(("Z","G3","G4","G5","R0","P0"))]
    v1_dx = baseline_billing.get("diagnosticCode", "")

    # Step 2: regenerate SOAP with Procedure section
    transcript_clipped = transcript[:32000]  # cap for context
    soap_v2_raw = call_llm(SOAP_MODEL, SOAP_PROCSEC_PROMPT, transcript_clipped, "soap_note")
    soap_v2_obj = extract_last_json_block(soap_v2_raw)
    if soap_v2_obj is None:
        return {
            "sid": sid, "name": name, "date": date,
            "v1_procedures": v1_procs, "v1_dx": v1_dx,
            "v2_procedure_section": "<SOAP REGEN FAILED>",
            "v2_procedures": None, "v2_dx": None,
            "soap_v2_raw_excerpt": soap_v2_raw[:500],
        }
    procedure_entries = soap_v2_obj.get("procedure", [])
    v2_soap_text = soap_json_to_text(soap_v2_obj)

    # Step 3: validate the cited quote is (a) actually in the transcript,
    # (b) past-tense / completion-phrasing — NOT modal-future ("can inject",
    # "could spray"), narrated-future ("going to", "let me grab", "we'll"),
    # or offered ("offered", "considered"). The Apr 24 simulation showed
    # substring-only validation accepts "Let me grab some liquid nitrogen"
    # (Martin) and "I can inject the shoulder" (Mary Beth) — both false
    # positives. This filter rejects them.
    MODAL_FUTURE = [
        r"\bi can\b", r"\bwe can\b", r"\bcould\b", r"\bwould\b",
        r"\bmight\b", r"\bmay\b",
        r"\bgoing to\b", r"\bgonna\b", r"\bgoing\s+to\b",
        r"\blet me\b", r"\blet's\b", r"\bwe'll\b", r"\bi'll\b", r"\bi will\b",
        r"\bschedul", r"\barrange", r"\bconsider",
        r"\boffered\b", r"\bofferin",
        r"\binstead\b", r"\bover-the-counter\b",
    ]
    PAST_OR_DONE = [
        r"\bi (?:did|drew|gave|injected|sprayed|applied|removed|biopsied|excised|sutured|drained|froze|placed|inserted)\b",
        r"\bi just\b", r"\bi already\b", r"\bdone\b",
        r"\b(?:was|were|been) (?:injected|administered|drawn|applied|sprayed|frozen|drained|removed|sutured|excised|placed)\b",
        r"\b(?:injected|administered|applied|drew|sprayed|removed|excised|sutured|drained|froze|placed) (?:it|the|her|his|that)\b",
        r"\benter on the\b",            # "I'm gonna enter on the right side" — V2 narration mid-procedure
        r"\bput bandaid\b", r"\bput.*bandage\b",
        r"\bbandaid on\b",
    ]

    def quote_is_past(q: str) -> bool:
        ql = q.lower()
        if any(re.search(p, ql) for p in MODAL_FUTURE):
            return False
        return any(re.search(p, ql) for p in PAST_OR_DONE)

    transcript_lc = transcript.lower()
    validated_procs = []
    rejected_procs = []
    for p in procedure_entries:
        q = (p.get("transcript_quote") or "").strip()
        ql = q.lower()
        in_transcript = bool(q) and ql in transcript_lc
        looks_past   = quote_is_past(q)
        if in_transcript and looks_past:
            validated_procs.append(p)
        else:
            rejected_procs.append({
                "action": p.get("action",""),
                "quote": q,
                "reason": ("not_in_transcript" if not in_transcript else "modal_or_future"),
            })

    # Rebuild SOAP text with only validated procedures (kept-quote check enforces step 3)
    soap_v2_obj_validated = dict(soap_v2_obj)
    soap_v2_obj_validated["procedure"] = validated_procs
    v2_soap_text_validated = soap_json_to_text(soap_v2_obj_validated)

    # Run billing extraction on the validated SOAP
    bill_user = f"## SOAP Note (5-section S/O/A/P/Procedure)\n\n{v2_soap_text_validated}\n\n## Full Transcript\n\n{transcript[-4500:]}"
    bill_v2_raw = call_llm(BILL_MODEL, BILL_V2_PROMPT, bill_user, "billing_extraction")
    bill_v2_obj = extract_last_json_block(bill_v2_raw)
    v2_procs = bill_v2_obj.get("procedures", []) if bill_v2_obj else None
    v2_dx = bill_v2_obj.get("suggestedDiagnosticCode", "") if bill_v2_obj else None

    return {
        "sid": sid, "name": name, "date": date,
        "v1_procedures": v1_procs, "v1_dx": v1_dx,
        "v2_procedure_section_count": len(procedure_entries),
        "v2_procedure_entries": [{"action": p.get("action",""), "quote": (p.get("transcript_quote","")[:120])} for p in procedure_entries],
        "v2_validated_count": len(validated_procs),
        "v2_rejected_quotes": rejected_procs,
        "v2_procedures": v2_procs,
        "v2_dx": v2_dx,
    }

def main():
    out = []
    # Run sequentially to keep router happy and avoid mixing logs.
    for sid, name, date in SESSIONS:
        print(f"--- {sid}  {name}  ({date}) ---", flush=True)
        r = process_session(sid, name, date)
        out.append(r)
        print(f"  V1 procedures: {r['v1_procedures']}")
        print(f"  V2 SOAP procedure section: {r.get('v2_procedure_section_count')} entries, {r.get('v2_validated_count')} survived transcript-citation check")
        if r.get("v2_rejected_quotes"):
            print(f"  V2 REJECTED (citation not in transcript):")
            for rj in r["v2_rejected_quotes"]:
                print(f"     - {rj['action'][:70]}  quote={rj['quote'][:80]!r}")
        print(f"  V2 billing procedures: {r['v2_procedures']}")
        print()

    with open("/tmp/sim_e2e/results.json", "w") as f:
        json.dump(out, f, indent=2)
    print("\nWritten to /tmp/sim_e2e/results.json")

if __name__ == "__main__":
    main()
