#!/usr/bin/env python3
"""Phase 1: regenerate SOAPs for the 13 sessions using the freshly-landed
v0.10.61 SOAP prompt. Output: /tmp/sim_billing/soap_v061_<sid>.json
(parsed JSON object with subjective/objective/assessment/plan/procedure)."""

import json, urllib.request, base64, re
from concurrent.futures import ThreadPoolExecutor, as_completed

LLM_URL = "http://100.119.83.76:8080/v1/chat/completions"
LLM_KEY = "ai-scribe-secret-key"
SOAP_MODEL = "soap-model-fast"
CLIENT  = "ai-scribe"

SOAP_PROMPT = open("/tmp/sim_billing/soap_v061_prompt.txt").read()

SESSIONS = [
    "0cce8568", "58e617ac", "e11d3112", "173c4253", "8feb77a1",
    "1ec7a57e", "98821f67", "b64ff7f3", "de7f5122", "491c6493",
    "bc81d25e", "eeb4d85f", "ccc2b9fb",
]

def call(system, user, timeout=180):
    payload = json.dumps({"model":SOAP_MODEL,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"temperature":0.1}).encode()
    req = urllib.request.Request(LLM_URL, data=payload, headers={
        "Content-Type":"application/json","Authorization":f"Bearer {LLM_KEY}",
        "X-Client-Id":CLIENT,"X-Clinic-Task":"soap_note"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())["choices"][0]["message"]["content"]

def extract_last_json_block(text):
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

def regen(sid):
    tr = base64.b64decode(open(f"/tmp/sim_billing/transcript_{sid}.b64").read()).decode()[:32000]
    raw = call(SOAP_PROMPT, tr)
    obj = extract_last_json_block(raw)
    if obj is None:
        print(f"  {sid}: PARSE FAIL — saving raw", flush=True)
        with open(f"/tmp/sim_billing/soap_v061_{sid}.raw.txt","w") as f: f.write(raw)
        return sid, None
    with open(f"/tmp/sim_billing/soap_v061_{sid}.json","w") as f: json.dump(obj, f, indent=2)
    n_proc = len(obj.get("procedure", []))
    n_cand = len(obj.get("procedure_candidates", []))
    print(f"  {sid}: ok — procedure={n_proc}, candidates={n_cand}", flush=True)
    return sid, obj

def main():
    with ThreadPoolExecutor(max_workers=2) as ex:
        list(ex.map(regen, SESSIONS))
    print("DONE — SOAPs regenerated")

if __name__ == "__main__":
    main()
