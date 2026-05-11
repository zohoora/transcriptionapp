#!/usr/bin/env python3
"""
Backfill soap_patient_*.txt files to the profile service.

Context: Before the ALLOWED_SESSION_FILES allowlist included soap_patient_*.txt,
multi-patient sessions uploaded patient_labels.json but the per-patient SOAP
files were silently rejected (HTTP 400). The server-side get_session() then
returned patientNotes entries with empty content, and the frontend rendered
a blank SOAP body.

This script walks ~/.transcriptionapp/archive/, finds sessions that have
patient_labels.json + soap_patient_N.txt files locally, and PUTs each file
to the profile service. Idempotent — re-runs upload the same content.

Usage:
    # Dry run (default) — list what would be uploaded
    python3 scripts/backfill_per_patient_soap.py

    # Apply (actually upload)
    python3 scripts/backfill_per_patient_soap.py --apply

    # Limit to a single session_id (full or 8-char prefix)
    python3 scripts/backfill_per_patient_soap.py --apply --session 7375a8dd

Run on each workstation that hosts affected sessions in its local archive
(check both Room 6 and Room 2). Run from any directory.
"""

import argparse
import glob
import json
import os
import sys
import urllib.error
import urllib.request


ARCHIVE_ROOT = os.path.expanduser("~/.transcriptionapp/archive")
ROOM_CONFIG = os.path.expanduser("~/.transcriptionapp/room_config.json")


def load_server_url():
    with open(ROOM_CONFIG) as f:
        cfg = json.load(f)
    return cfg["profile_server_url"].rstrip("/")


def upload_file(server_url: str, phys_id: str, sid: str, filename: str, body: bytes) -> tuple[int, str]:
    url = f"{server_url}/physicians/{phys_id}/sessions/{sid}/files/{filename}"
    req = urllib.request.Request(url, data=body, method="PUT")
    req.add_header("Content-Type", "text/plain; charset=utf-8")
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.status, resp.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", errors="replace")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true", help="Actually upload (default: dry run)")
    ap.add_argument("--session", help="Limit to one session_id (full or 8-char prefix)")
    args = ap.parse_args()

    server_url = load_server_url()
    print(f"Server: {server_url}")
    print(f"Archive root: {ARCHIVE_ROOT}")
    print(f"Mode: {'APPLY' if args.apply else 'DRY RUN'}")
    print()

    pattern = os.path.join(ARCHIVE_ROOT, "*/*/*/*/patient_labels.json")
    found = 0
    uploaded = 0
    skipped = 0
    failed = 0

    for labels_path in sorted(glob.glob(pattern)):
        session_dir = os.path.dirname(labels_path)
        sid = os.path.basename(session_dir)

        if args.session and not sid.startswith(args.session):
            continue

        # Need metadata.json for physician_id
        meta_path = os.path.join(session_dir, "metadata.json")
        if not os.path.isfile(meta_path):
            continue
        with open(meta_path) as f:
            meta = json.load(f)
        phys_id = meta.get("physician_id")
        if not phys_id:
            print(f"  SKIP {sid[:8]} — no physician_id in metadata")
            skipped += 1
            continue

        # Find all soap_patient_*.txt
        patient_files = sorted(glob.glob(os.path.join(session_dir, "soap_patient_*.txt")))
        if not patient_files:
            continue

        found += 1
        name = meta.get("patient_name") or "<no name>"
        print(f"{sid[:8]} | {name} | {len(patient_files)} per-patient files")

        for pf in patient_files:
            fname = os.path.basename(pf)
            size = os.path.getsize(pf)
            if size == 0:
                print(f"  - {fname} ({size}B) — SKIP empty")
                skipped += 1
                continue
            if not args.apply:
                print(f"  - {fname} ({size}B) — would upload")
                continue
            with open(pf, "rb") as f:
                body = f.read()
            status, resp = upload_file(server_url, phys_id, sid, fname, body)
            if status in (200, 201, 204):
                print(f"  - {fname} ({size}B) — uploaded OK")
                uploaded += 1
            else:
                print(f"  - {fname} ({size}B) — FAILED HTTP {status}: {resp[:120]}")
                failed += 1

    print()
    print(f"Sessions found with per-patient files: {found}")
    if args.apply:
        print(f"Files uploaded: {uploaded}")
        print(f"Files skipped: {skipped}")
        print(f"Files failed: {failed}")
    else:
        print("Dry run — re-run with --apply to upload.")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
