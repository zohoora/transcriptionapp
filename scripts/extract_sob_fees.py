#!/usr/bin/env python3
"""
Extract OHIP fee codes and rates from the Schedule of Benefits PDF.

Reads the pdftotext output and extracts every fee code with its description
and dollar amount. Outputs a clean JSON file that can be used as ground truth
for the billing database.

Usage:
    pdftotext -layout SOB.pdf /tmp/sob.txt
    python3 scripts/extract_sob_fees.py /tmp/sob.txt > /tmp/sob_codes.json
"""

import sys
import re
import json
from collections import defaultdict

def extract_codes(filepath):
    with open(filepath, 'r') as f:
        text = f.read()

    lines = text.split('\n')
    codes = {}

    # Pattern 1: "CODE Description....... AMOUNT" (main fee table format)
    # e.g., "A003 General assessment...............    95.60"
    pat1 = re.compile(r'^\s*([A-Z]\d{3})\s+(.+?)\s*\.{2,}\s*(\d+\.\d{2})\s*$')

    # Pattern 2: "CODE Description .... AMOUNT" with fewer dots
    pat2 = re.compile(r'^\s*([A-Z]\d{3})\s+(.+?)\s+(\d+\.\d{2})\s*$')

    # Pattern 3: Numeric index format "CODE    AMOUNT    PAGE"
    pat3 = re.compile(r'^\s*([A-Z]\d{3})\s+(\d+\.\d{2})\s+[A-Z]\d+\s*$')

    # Pattern 4: "add" lines like "...to A001 and A007 ....... add    5.00"
    pat_add = re.compile(r'to\s+([A-Z]\d{3}).*?add\s+(\d+\.\d{2})')

    for i, line in enumerate(lines):
        # Skip empty lines
        if not line.strip():
            continue

        # Try pattern 1 (dotted)
        m = pat1.match(line)
        if m:
            code, desc, amount = m.group(1), m.group(2).strip().rstrip('.'), m.group(3)
            if code not in codes or len(desc) > len(codes[code].get('description', '')):
                codes[code] = {
                    'code': code,
                    'description': desc.strip(),
                    'fee': float(amount),
                    'line': i + 1
                }
            continue

        # Try pattern 2 (no dots, but amount at end)
        m = pat2.match(line)
        if m:
            code, desc, amount = m.group(1), m.group(2).strip(), m.group(3)
            # Filter out false positives (lines that are just commentary)
            if len(desc) > 5 and not desc.startswith('is ') and not desc.startswith('In '):
                if code not in codes:
                    codes[code] = {
                        'code': code,
                        'description': desc.strip(),
                        'fee': float(amount),
                        'line': i + 1
                    }
            continue

        # Try pattern 3 (numeric index)
        m = pat3.match(line)
        if m:
            code, amount = m.group(1), m.group(2)
            if code not in codes:
                codes[code] = {
                    'code': code,
                    'description': '',
                    'fee': float(amount),
                    'line': i + 1
                }

    return codes

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 extract_sob_fees.py <sob_text_file>", file=sys.stderr)
        sys.exit(1)

    codes = extract_codes(sys.argv[1])

    # Filter to codes we care about (Family Practice billable)
    # A-codes, B-codes, C-codes, E-codes, G-codes, H-codes, J-codes, K-codes, P-codes, Q-codes, R-codes, Z-codes
    relevant_prefixes = set('ABCEGHJ KPQRZ'.split())  # All relevant

    # Sort by code
    sorted_codes = sorted(codes.values(), key=lambda c: c['code'])

    # Print specific codes we need
    needed = [
        'A001', 'A002', 'A003', 'A004', 'A005', 'A006', 'A007', 'A008',
        'A101', 'A102', 'A110', 'A112', 'A777', 'A888', 'A900', 'A901', 'A902', 'A903', 'A905',
        'A191', 'A192', 'A193', 'A194', 'A195',
        'A917', 'A927', 'A937', 'A947', 'A957', 'A967',
        'A990', 'A994', 'A996', 'A998',
        'B990', 'B992', 'B993', 'B994', 'B996', 'B998',
        'C001', 'C002', 'C003', 'C004', 'C009', 'C010', 'C012', 'C882', 'C903',
        'E079', 'E082', 'E430', 'E431', 'E542',
        'G001', 'G002', 'G004', 'G005', 'G009', 'G010', 'G011', 'G012', 'G014',
        'G123', 'G197', 'G202', 'G205', 'G209', 'G212',
        'G223', 'G227', 'G228', 'G231', 'G235',
        'G271', 'G310', 'G313',
        'G365', 'G370', 'G371', 'G372', 'G373', 'G375', 'G377', 'G378', 'G379', 'G381',
        'G384', 'G385', 'G394',
        'G420', 'G435', 'G462',
        'G481', 'G482', 'G489', 'G525',
        'G538', 'G552', 'G590',
        'G840', 'G841', 'G842', 'G843', 'G844', 'G845', 'G846', 'G847', 'G848',
        'H003', 'H004',
        'J301', 'J304', 'J324', 'J327',
        'K001', 'K002', 'K003', 'K004', 'K005', 'K006', 'K007', 'K008',
        'K013', 'K015', 'K017',
        'K022', 'K023', 'K028', 'K029', 'K030', 'K031', 'K032', 'K033', 'K034', 'K035',
        'K036', 'K037', 'K038', 'K039',
        'K070', 'K071',
        'K130', 'K131', 'K132', 'K133',
        'K140', 'K141', 'K142', 'K143', 'K144',
        'K655', 'K656',
        'K700', 'K702', 'K730', 'K731', 'K732', 'K733', 'K738', 'K998',
        'P001', 'P002', 'P003', 'P004', 'P005', 'P006', 'P007', 'P008', 'P009',
        'P013', 'P014', 'P018',
        'Q010', 'Q012', 'Q015', 'Q040', 'Q042', 'Q050', 'Q053', 'Q054',
        'Q100', 'Q101', 'Q102', 'Q200', 'Q888',
        'R048', 'R051', 'R094',
        'Z101', 'Z110', 'Z113', 'Z114', 'Z116', 'Z117',
        'Z122', 'Z125', 'Z128', 'Z129',
        'Z154', 'Z156', 'Z157', 'Z158', 'Z159', 'Z160', 'Z161', 'Z162',
        'Z175', 'Z176',
        'Z314', 'Z315',
        'Z535', 'Z543', 'Z545', 'Z611', 'Z847',
    ]

    print("=" * 70)
    print("OHIP SCHEDULE OF BENEFITS — April 2026 Fee Extraction")
    print("=" * 70)

    found = 0
    missing = []
    for code in sorted(needed):
        if code in codes:
            c = codes[code]
            desc = c['description'][:60] if c['description'] else '(no description extracted)'
            print(f"  {code}  ${c['fee']:>8.2f}  {desc}")
            found += 1
        else:
            missing.append(code)

    print(f"\n  Found: {found}/{len(needed)}")
    if missing:
        print(f"  Missing: {', '.join(missing)}")

    # Also output as JSON for programmatic use
    json_output = {}
    for code in needed:
        if code in codes:
            json_output[code] = codes[code]['fee']

    with open('/tmp/sob_fees.json', 'w') as f:
        json.dump(json_output, f, indent=2, sort_keys=True)
    print(f"\n  JSON written to /tmp/sob_fees.json")

if __name__ == '__main__':
    main()
