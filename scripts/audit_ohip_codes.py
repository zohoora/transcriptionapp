#!/usr/bin/env python3
"""
Comprehensive OHIP Code Audit Script

Extracts ALL fee codes from the Schedule of Benefits PDF, cross-references
against the current billing database, identifies missing codes that a GP
without special designation can bill, and verifies basket/fee accuracy.

Usage: python3 scripts/audit_ohip_codes.py
"""

import re, json, subprocess, sys
from collections import Counter, defaultdict
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
SOB_PDF = Path("/Users/backoffice/oma/moh-schedule-benefit-2026-03-27.pdf")
FHO_PDF = Path("/Users/backoffice/oma/fho-contract.pdf")
OHIP_CODES_RS = PROJECT_ROOT / "tauri-app/src-tauri/src/billing/ohip_codes.rs"

# ── PDF extraction ───────────────────────────────────────────────────────────

def pdf_to_text(path, layout=True):
    args = ["pdftotext"] + (["-layout"] if layout else []) + [str(path), "-"]
    r = subprocess.run(args, capture_output=True, text=True, timeout=60)
    return r.stdout

# ── Phase 1: Extract ALL codes from SOB with page-section mapping ────────────

def extract_sob_codes(text):
    lines = text.split('\n')

    # Build line→SOB-page mapping from page footers
    page_at_line = {}
    for i, line in enumerate(lines):
        if 'March 9' in line or 'Amd 12' in line:
            for m in re.findall(r'\b([A-Z]{1,2}\d{1,3})\b', line):
                if re.match(r'^(A|GP|SP|J|R|Z|P|Q|K|E|H)\d+$', m):
                    page_at_line[i] = m
                    break

    # Extract codes with fees, mapping each to its nearest SOB page section
    codes = {}
    code_pattern = re.compile(r'#?\s*([A-Z]\d{3}[A-Z]?)\s+(.+?)\s+(\d+\.\d{2})\s*$')

    for i, line in enumerate(lines):
        m = code_pattern.match(line.lstrip())
        if not m:
            # Try: code + fee on same line, description may be minimal
            m2 = re.match(r'[\s#]*([A-Z]\d{3}[A-Z]?)\s+.*?(\d+\.\d{2})\s*$', line)
            if m2:
                code = m2.group(1)
                fee = float(m2.group(2))
                desc = line[line.index(code)+len(code):line.rindex(m2.group(2))].strip().rstrip('.').strip()
            else:
                continue
        else:
            code = m.group(1)
            desc = m.group(2).strip().rstrip('.').strip()
            fee = float(m.group(3))

        if fee <= 0:
            continue

        # Find SOB section from nearest page marker above
        section = 'Unknown'
        for search in range(i, max(i - 80, 0), -1):
            if search in page_at_line:
                section = re.match(r'([A-Z]+)', page_at_line[search]).group(1)
                break

        has_hash = line.lstrip().startswith('#')

        if code not in codes or fee > codes[code]['fee']:
            codes[code] = {
                'fee': fee,
                'description': desc[:100],
                'section': section,
                'has_hash': has_hash,
            }

    return codes

# ── Phase 2: FHO basket extraction ──────────────────────────────────────────

def extract_fho_basket(text):
    lines = text.split('\n')
    sched2_codes = set()
    sched2b_codes = set()

    # Find APPENDIX I sections by line number
    s2_start = s2a_start = s2b_start = s3_start = None
    for i, line in enumerate(lines):
        if 'APPENDIX I' in line and 'SCHEDULE 2' in line and '2A' not in line and '2B' not in line:
            s2_start = i
        elif 'SCHEDULE 2A' in line and i > 3000:
            s2a_start = i
        elif 'SCHEDULE 2B' in line and i > 3000:
            s2b_start = i
        elif 'SCHEDULE 3' in line and i > 3000:
            s3_start = i

    if s2_start:
        end = s2a_start or s2b_start or len(lines)
        for line in lines[s2_start:end]:
            for code in re.findall(r'([A-Z]\d{3}[A-Z])', line):
                sched2_codes.add(code)

    if s2b_start:
        end = s3_start or len(lines)
        for line in lines[s2b_start:end]:
            for code in re.findall(r'([A-Z]\d{3}[A-Z])', line):
                sched2b_codes.add(code)

    return sched2_codes | sched2b_codes, sched2_codes - sched2b_codes, sched2b_codes

# ── Phase 3: Load current database ──────────────────────────────────────────

def load_db():
    text = OHIP_CODES_RS.read_text()
    codes = {}
    for m in re.finditer(
        r'code:\s*"([A-Z]\d{3}[A-Z]?)".*?description:\s*"([^"]*)".*?ffs_rate_cents:\s*(\d+).*?basket:\s*Basket::(In|Out)',
        text, re.DOTALL
    ):
        codes[m.group(1)] = {
            'description': m.group(2),
            'fee_cents': int(m.group(3)),
            'basket': m.group(4),
        }
    return codes

# ── Phase 4: GP-billability classification ───────────────────────────────────

# SOB section A pages for Family Practice (00) are A1–A66.
# A67+ is Virtual Care (open to all), then specialty sections start at A77+
# Specialty code number ranges from the TOC:
SPECIALTY_A_PAGES = {
    77: 'Anaesthesia (01)', 80: 'Cardiology (60)', 82: 'Cardiac Surgery (09)',
    84: 'Clinical Immunology (62)', 86: 'Community Medicine (05)',
    90: 'Critical Care Medicine (11)', 92: 'Dermatology (02)',
    98: 'Emergency Medicine (12)', 99: 'Endocrinology (15)',
    105: 'Gastroenterology (41)', 108: 'General Surgery (03)',
    110: 'General Thoracic Surgery (64)', 112: 'Genetics (22)',
    119: 'Geriatrics (07)', 124: 'Haematology (61)',
    126: 'Infectious Disease (46)', 130: 'Internal Medicine (13)',
    133: 'Laboratory Medicine (28)', 135: 'Medical Oncology (44)',
    137: 'Nephrology (16)', 139: 'Neurology (18)',
    141: 'Neurosurgery (19)', 144: 'Nuclear Medicine (63)',
    146: 'Obs & Gynae (20)', 152: 'Ophthalmology (23)',
    156: 'Orthopaedic Surgery (24)', 160: 'Otolaryngology (25)',
    164: 'Paediatrics (26)', 168: 'Physical Medicine (27)',
    172: 'Plastic Surgery (30)', 175: 'Psychiatry (31)',
    179: 'Diagnostic Radiology (32)', 182: 'Radiation Oncology (34)',
    186: 'Respiratory Disease (47)', 188: 'Rheumatology (48)',
    190: 'Urology (36)', 193: 'Vascular Surgery (37)',
}

def classify_gp_billable(code, info, sob_text_lines, page_at_line_map):
    """Returns (is_gp_billable: bool, reason: str)"""
    section = info['section']

    # Section J (D&T Procedures), R/Z (Surgical), P (Obstetrical),
    # K, Q, E codes: open to any physician
    if section in ('J', 'R', 'Z', 'P', 'Q', 'K', 'E', 'H'):
        return True, f"Section {section} — open to all physicians"

    # Section A: need to check if it's Family Practice (A1-A66) or specialist (A77+)
    if section == 'A':
        # Find the actual page number for this code
        # Look up in page_at_line_map
        for search in range(info.get('_line', 0), max(info.get('_line', 0) - 80, 0), -1):
            if search in page_at_line_map:
                page_str = page_at_line_map[search]
                page_num_m = re.search(r'A(\d+)', page_str)
                if page_num_m:
                    page_num = int(page_num_m.group(1))
                    if page_num <= 76:
                        return True, f"Section A page {page_num} — Family Practice / Virtual Care"
                    else:
                        # Find which specialty
                        specialty = "Unknown specialty"
                        for sp_page, sp_name in sorted(SPECIALTY_A_PAGES.items()):
                            if page_num >= sp_page:
                                specialty = sp_name
                        return False, f"Section A page {page_num} — {specialty}"
                break

        # Couldn't determine page — check code prefix heuristics
        # A0xx codes are typically Family Practice
        code_num = int(re.search(r'\d{3}', code).group())
        if code[0] == 'A' and code_num <= 8:
            return True, "A00x code — Family Practice assessment"
        return None, "Section A — page unknown, needs manual review"

    # General Preamble / Surgical Preamble codes
    if section in ('GP', 'SP'):
        return None, f"Section {section} — preamble, likely not directly billable"

    return None, f"Section '{section}' — needs manual review"

# ── Phase 5: Companion code extraction ───────────────────────────────────────

def extract_companions(text):
    companions = []
    limits = []
    lines = text.split('\n')

    for i, line in enumerate(lines):
        # "add" fee lines
        if re.search(r'\badd\b', line, re.IGNORECASE) and re.search(r'\d+\.\d{2}', line):
            codes = re.findall(r'([A-Z]\d{3}[A-Z]?)', line)
            fee_m = re.search(r'(\d+\.\d{2})', line)
            if len(codes) >= 1 and fee_m:
                companions.append({
                    'addon': codes[0],
                    'bases': codes[1:],
                    'fee': float(fee_m.group(1)),
                    'context': line.strip()[:120],
                })

        # Frequency limits
        lm = re.search(r'limited to (?:a maximum of )?(\d+) services? per patient per (\d+\s+month|\w+)', line, re.IGNORECASE)
        if lm:
            ctx_codes = re.findall(r'([A-Z]\d{3}[A-Z]?)', '\n'.join(lines[max(0,i-3):i+1]))
            limits.append({'max': int(lm.group(1)), 'period': lm.group(2), 'codes': ctx_codes})

    return companions, limits

# ── Report generation ────────────────────────────────────────────────────────

def main():
    print("OHIP Code Audit — Starting...\n")

    # Extract texts
    sob_text = pdf_to_text(SOB_PDF, layout=True)
    fho_text = pdf_to_text(FHO_PDF, layout=False)
    sob_lines = sob_text.split('\n')

    # Build page mapping
    page_at_line = {}
    for i, line in enumerate(sob_lines):
        if 'March 9' in line or 'Amd 12' in line:
            for m in re.findall(r'\b([A-Z]{1,2}\d{1,3})\b', line):
                if re.match(r'^(A|GP|SP|J|R|Z|P|Q|K|E|H)\d+$', m):
                    page_at_line[i] = m
                    break

    # Phase 1
    print("Phase 1: Extracting codes from SOB...")
    sob_codes = extract_sob_codes(sob_text)
    # Store line numbers for GP classification
    code_pattern = re.compile(r'#?\s*([A-Z]\d{3}[A-Z]?)\s')
    for i, line in enumerate(sob_lines):
        m = code_pattern.match(line.lstrip())
        if m and m.group(1) in sob_codes:
            sob_codes[m.group(1)]['_line'] = i
    print(f"  {len(sob_codes)} codes with fees extracted")

    # Phase 2
    print("Phase 2: Extracting FHO basket...")
    basket_all, basket_30, basket_50 = extract_fho_basket(fho_text)
    print(f"  Basket: {len(basket_all)} total (Sched 2: {len(basket_30)}, Sched 2B: {len(basket_50)})")

    # Phase 3
    print("Phase 3: Loading current database...")
    db = load_db()
    print(f"  {len(db)} codes in database")

    # Phase 4
    print("Phase 4: Classifying GP-billability...")
    companions, limits = extract_companions(sob_text)
    print(f"  {len(companions)} companion relationships, {len(limits)} frequency limits")

    # Cross-reference
    missing_gp = []
    missing_specialist = []
    missing_review = []
    fee_mismatches = []
    basket_mismatches = []

    for code, info in sorted(sob_codes.items()):
        # Skip if already in DB
        if code in db:
            continue
        # Skip suffix-less codes when suffixed version exists
        if len(code) == 4 and code + 'A' in db:
            continue

        gp, reason = classify_gp_billable(code, info, sob_lines, page_at_line)
        in_basket = code in basket_all or (code + 'A' if len(code) == 4 else '') in basket_all
        shadow = 50 if code in basket_50 else (30 if code in basket_30 else (100 if not in_basket else 30))

        entry = {
            'code': code,
            'fee': info['fee'],
            'description': info['description'],
            'section': info['section'],
            'in_basket': in_basket,
            'shadow_pct': shadow,
            'gp_billable': gp,
            'reason': reason,
        }

        if gp is True:
            missing_gp.append(entry)
        elif gp is False:
            missing_specialist.append(entry)
        else:
            missing_review.append(entry)

    # Fee verification for existing codes
    for code, db_info in sorted(db.items()):
        sob_info = sob_codes.get(code) or sob_codes.get(code[:-1] if len(code) == 5 else code)
        if sob_info:
            sob_cents = int(round(sob_info['fee'] * 100))
            if abs(sob_cents - db_info['fee_cents']) > 1:
                fee_mismatches.append({
                    'code': code, 'db': db_info['fee_cents'], 'sob': sob_cents,
                })
        expected = 'In' if code in basket_all else 'Out'
        if db_info['basket'] != expected:
            basket_mismatches.append({
                'code': code, 'db': db_info['basket'], 'fho': expected,
            })

    # ── Build report ─────────────────────────────────────────────────────────
    R = []
    R.append("=" * 80)
    R.append("OHIP CODE AUDIT REPORT — April 2026 SOB vs Current Database")
    R.append(f"Database: {len(db)} codes | SOB extracted: {len(sob_codes)} codes with fees")
    R.append(f"FHO basket: {len(basket_all)} codes (Sched 2: {len(basket_30)}, 2B: {len(basket_50)})")
    R.append("=" * 80)

    R.append(f"\n{'='*80}")
    R.append(f"SECTION 1: MISSING GP-BILLABLE CODES — {len(missing_gp)} found")
    R.append(f"Billable by any physician. NOT in our database. Grouped by SOB section.")
    R.append(f"{'='*80}")

    by_section = defaultdict(list)
    for e in missing_gp:
        by_section[e['section']].append(e)

    for section in sorted(by_section.keys()):
        entries = by_section[section]
        R.append(f"\n  --- Section {section} ({len(entries)} codes) ---")
        for e in entries:
            basket_str = f"IN({e['shadow_pct']}%)" if e['in_basket'] else "OUT(FFS)"
            R.append(f"    {e['code']:6s} ${e['fee']:>8.2f}  {basket_str:10s}  {e['description'][:60]}")

    R.append(f"\n{'='*80}")
    R.append(f"SECTION 2: FEE MISMATCHES — {len(fee_mismatches)} found")
    R.append(f"{'='*80}")
    for m in fee_mismatches:
        R.append(f"  {m['code']:6s}  DB: ${m['db']/100:.2f}  SOB: ${m['sob']/100:.2f}  Diff: {m['sob']-m['db']:+d}¢")

    R.append(f"\n{'='*80}")
    R.append(f"SECTION 3: BASKET MISMATCHES — {len(basket_mismatches)} found")
    R.append(f"{'='*80}")
    for m in basket_mismatches:
        R.append(f"  {m['code']:6s}  DB: {m['db']}  FHO: {m['fho']}")

    R.append(f"\n{'='*80}")
    R.append(f"SECTION 4: SPECIALIST-ONLY — {len(missing_specialist)} (not billable by GP)")
    R.append(f"{'='*80}")
    by_spec = defaultdict(list)
    for e in missing_specialist:
        by_spec[e['reason']].append(e)
    for reason in sorted(by_spec.keys()):
        R.append(f"\n  {reason} ({len(by_spec[reason])} codes)")
        for e in by_spec[reason][:5]:
            R.append(f"    {e['code']:6s} ${e['fee']:>8.2f}  {e['description'][:50]}")
        if len(by_spec[reason]) > 5:
            R.append(f"    ... +{len(by_spec[reason])-5} more")

    R.append(f"\n{'='*80}")
    R.append(f"SECTION 5: NEEDS MANUAL REVIEW — {len(missing_review)}")
    R.append(f"{'='*80}")
    for e in missing_review[:30]:
        R.append(f"  {e['code']:6s} ${e['fee']:>8.2f}  {e['section']:8s}  {e['reason'][:50]}  {e['description'][:40]}")
    if len(missing_review) > 30:
        R.append(f"  ... +{len(missing_review)-30} more")

    R.append(f"\n{'='*80}")
    R.append("SUMMARY")
    R.append(f"{'='*80}")
    R.append(f"  Missing GP-billable:     {len(missing_gp)}")
    R.append(f"  Fee mismatches:          {len(fee_mismatches)}")
    R.append(f"  Basket mismatches:       {len(basket_mismatches)}")
    R.append(f"  Specialist-only:         {len(missing_specialist)}")
    R.append(f"  Needs manual review:     {len(missing_review)}")

    # Verify G246 is found
    g246 = [e for e in missing_gp if e['code'] == 'G246']
    R.append(f"\n  VALIDATION: G246 found in missing GP-billable? {'YES' if g246 else 'NO — PROBLEM'}")

    report = '\n'.join(R)
    Path("/tmp/ohip_audit_report.txt").write_text(report)
    Path("/tmp/ohip_missing_codes.json").write_text(json.dumps({
        'missing_gp_billable': missing_gp,
        'fee_mismatches': fee_mismatches,
        'basket_mismatches': basket_mismatches,
    }, indent=2))

    print(f"\nReport: /tmp/ohip_audit_report.txt")
    print(f"JSON:   /tmp/ohip_missing_codes.json")
    print(f"\n{report[report.index('SUMMARY'):]}")

if __name__ == '__main__':
    main()
