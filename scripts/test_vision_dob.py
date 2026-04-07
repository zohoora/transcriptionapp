#!/usr/bin/env python3
"""
Test whether the vision model can extract both patient name AND date of birth
from EMR screenshots.

Sends screenshots to the vision-model with a modified prompt that asks for
both name and DOB, then evaluates accuracy.

Usage:
    python3 scripts/test_vision_dob.py
"""

import json
import os
import sys
import base64
import requests
import glob

# ═══════════════════════════════════════════════════════════════════
# CONFIG
# ═══════════════════════════════════════════════════════════════════
LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
MODEL = "vision-model"

def load_config():
    config_path = os.path.expanduser("~/.transcriptionapp/config.json")
    with open(config_path) as f:
        config = json.load(f)
    return config.get("llm_api_key", ""), config.get("llm_client_id", "ami-assist")

# ═══════════════════════════════════════════════════════════════════
# PROMPTS TO TEST
# ═══════════════════════════════════════════════════════════════════

# Current prompt (name only)
PROMPT_NAME_ONLY = {
    "system": "You are analyzing a screenshot of a computer screen in a clinical setting. If a patient's chart or medical record is clearly visible, extract the patient's full name. If no patient name is clearly visible, respond with NOT_FOUND.",
    "user": "Extract the patient name if one is clearly visible on screen. Respond with ONLY the patient name or NOT_FOUND. No explanation."
}

# New prompt (name + DOB)
PROMPT_NAME_DOB = {
    "system": "You are analyzing a screenshot of a computer screen in a clinical setting. If a patient's chart or medical record is clearly visible, extract the patient's full name and date of birth. Respond in exactly this format:\nNAME: <full name>\nDOB: <YYYY-MM-DD>\nIf name is not visible, respond: NAME: NOT_FOUND\nIf DOB is not visible, respond: DOB: NOT_FOUND",
    "user": "Extract the patient name and date of birth if visible on screen. Use the exact format:\nNAME: <name>\nDOB: <YYYY-MM-DD>\nRespond with NOT_FOUND for any field not clearly visible. No explanation."
}

# Alternative: JSON format
PROMPT_JSON = {
    "system": "You are analyzing a screenshot of a computer screen in a clinical setting. If a patient's chart or medical record is visible, extract the patient's full name and date of birth. Respond with ONLY a JSON object, no other text.",
    "user": 'Extract patient name and date of birth from this screenshot. Respond with ONLY:\n{"name": "<full name or NOT_FOUND>", "dob": "<YYYY-MM-DD or NOT_FOUND>"}'
}


def call_vision(system_prompt, user_prompt, image_path):
    """Call the vision model with an image"""
    api_key, client_id = load_config()

    with open(image_path, "rb") as f:
        image_b64 = base64.b64encode(f.read()).decode("utf-8")

    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": [
                {"type": "text", "text": user_prompt},
                {"type": "image_url", "image_url": {
                    "url": f"data:image/jpeg;base64,{image_b64}"
                }}
            ]}
        ],
        "temperature": 0.1,
        "max_tokens": 200,
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "X-Client-Id": client_id,
        "X-Clinic-Task": "vision_name_extraction",
        "Content-Type": "application/json",
    }

    try:
        resp = requests.post(LLM_URL, json=payload, headers=headers, timeout=60)
        resp.raise_for_status()
        return resp.json()["choices"][0]["message"]["content"]
    except Exception as e:
        return f"ERROR: {e}"


def parse_name_dob(response):
    """Parse NAME: / DOB: format"""
    name = None
    dob = None
    for line in response.strip().split("\n"):
        line = line.strip()
        if line.upper().startswith("NAME:"):
            val = line[5:].strip()
            name = val if val.upper() != "NOT_FOUND" else None
        elif line.upper().startswith("DOB:"):
            val = line[4:].strip()
            dob = val if val.upper() != "NOT_FOUND" else None
    return name, dob


def parse_json_response(response):
    """Parse JSON format"""
    try:
        text = response.strip()
        # Strip code fences
        text = text.replace("```json", "").replace("```", "").strip()
        start = text.find("{")
        end = text.rfind("}")
        if start == -1 or end == -1:
            return None, None
        obj = json.loads(text[start:end+1])
        name = obj.get("name")
        dob = obj.get("dob")
        if name and name.upper() == "NOT_FOUND":
            name = None
        if dob and dob.upper() == "NOT_FOUND":
            dob = None
        return name, dob
    except:
        return None, None


def find_test_screenshots():
    """Find screenshots from different patients"""
    screenshots = []
    archive_base = os.path.expanduser("~/.transcriptionapp/archive/2026/04")

    # Scan recent sessions for screenshots with known patient names
    for day_dir in sorted(glob.glob(f"{archive_base}/0[1-7]")):
        for session_dir in glob.glob(f"{day_dir}/*"):
            meta_path = os.path.join(session_dir, "metadata.json")
            screenshots_dir = os.path.join(session_dir, "screenshots")
            if not os.path.exists(meta_path) or not os.path.exists(screenshots_dir):
                continue
            try:
                with open(meta_path) as f:
                    meta = json.load(f)
                name = meta.get("patient_name", "")
                if not name:
                    continue
                # Pick a screenshot from the middle of the encounter
                imgs = sorted(glob.glob(f"{screenshots_dir}/*.jpg"))
                if len(imgs) < 3:
                    continue
                mid_img = imgs[len(imgs) // 2]
                screenshots.append({
                    "path": mid_img,
                    "expected_name": name,
                    "session_id": meta.get("session_id", ""),
                })
            except:
                continue

    # Deduplicate by patient name, take first
    seen = set()
    unique = []
    for s in screenshots:
        key = s["expected_name"].lower()
        if key not in seen:
            seen.add(key)
            unique.append(s)
        if len(unique) >= 6:
            break
    return unique


def main():
    screenshots = find_test_screenshots()
    if not screenshots:
        print("No screenshots found with patient names.")
        sys.exit(1)

    print("=" * 70)
    print("VISION MODEL DOB EXTRACTION TEST")
    print(f"Model: {MODEL}")
    print(f"Screenshots: {len(screenshots)}")
    print("=" * 70)

    # Test each prompt variant
    for prompt_name, prompt in [
        ("NAME+DOB (structured)", PROMPT_NAME_DOB),
        ("NAME+DOB (JSON)", PROMPT_JSON),
    ]:
        print(f"\n{'═' * 70}")
        print(f"PROMPT: {prompt_name}")
        print(f"{'═' * 70}")

        name_correct = 0
        dob_found = 0
        total = 0

        for ss in screenshots:
            total += 1
            print(f"\n  Screenshot: {os.path.basename(ss['path'])}")
            print(f"  Expected name: {ss['expected_name']}")

            response = call_vision(prompt["system"], prompt["user"], ss["path"])

            if response.startswith("ERROR:"):
                print(f"  ERROR: {response}")
                continue

            print(f"  Raw response: {response[:200]}")

            if prompt_name.endswith("(JSON)"):
                name, dob = parse_json_response(response)
            else:
                name, dob = parse_name_dob(response)

            print(f"  Parsed name: {name}")
            print(f"  Parsed DOB:  {dob}")

            # Check name accuracy (fuzzy — just check if expected name appears)
            if name:
                expected_parts = ss["expected_name"].lower().split()
                found_parts = name.lower().split()
                # At least 2 name parts match
                matching = sum(1 for p in expected_parts if any(p in f for f in found_parts))
                if matching >= 2:
                    name_correct += 1
                    print(f"  Name: ✓ MATCH")
                else:
                    print(f"  Name: ✗ MISMATCH")
            else:
                print(f"  Name: ✗ NOT_FOUND")

            if dob:
                dob_found += 1
                print(f"  DOB:  ✓ FOUND ({dob})")
            else:
                print(f"  DOB:  - NOT_FOUND")

        print(f"\n  {'─' * 50}")
        print(f"  Results for {prompt_name}:")
        print(f"    Name accuracy: {name_correct}/{total}")
        print(f"    DOB found:     {dob_found}/{total}")


if __name__ == "__main__":
    main()
