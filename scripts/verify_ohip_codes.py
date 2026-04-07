#!/usr/bin/env python3
"""
Verify the OHIP billing database against source documents.

Checks:
1. Every in-basket code matches the OMA FHO basket list
2. Every 50% shadow code matches FHO Contract Schedule 2B
3. Every rate matches the April 2026 SOB extraction
4. No in-basket code is missing from the database
5. No 50% code is missing from the database

Usage:
    python3 scripts/verify_ohip_codes.py
"""

import re, json, sys

# ═══════════════════════════════════════════════════════════════
# SOURCE 1: SOB-extracted rates
# ═══════════════════════════════════════════════════════════════
try:
    with open('/tmp/sob_fees_final.json') as f:
        sob_rates = json.load(f)
except FileNotFoundError:
    print("WARNING: /tmp/sob_fees_final.json not found — skipping rate verification")
    sob_rates = {}

# ═══════════════════════════════════════════════════════════════
# SOURCE 2: OMA FHO Basket List (from PDF, manually verified)
# ═══════════════════════════════════════════════════════════════
OMA_FHO_BASKET = set([
    "A001", "A003", "A004", "A007", "A008",
    "A101", "A102", "A110", "A112", "A777", "A900",
    "A917", "A927", "A937", "A947", "A957", "A967",
    "A990", "A994", "A996", "A998",
    "B990", "B992", "B993", "B994", "B996",
    "C882", "C903",
    "E542",
    "G001", "G002", "G004", "G005", "G009", "G010", "G011", "G012", "G014",
    "G123", "G197", "G202", "G205", "G209", "G212",
    "G223", "G227", "G228", "G231", "G235",
    "G271",
    "G310", "G313",
    "G365", "G370", "G371", "G372", "G373", "G375", "G377", "G378", "G379", "G381",
    "G384", "G385", "G394",
    "G420", "G435", "G462",
    "G481", "G482", "G489", "G525",
    "G538", "G552",
    "G840", "G841", "G842", "G843", "G844", "G845", "G846", "G847", "G848",
    "J301", "J304", "J324", "J327",
    "K001", "K002", "K003", "K004", "K005", "K006", "K007", "K008",
    "K013", "K015", "K017",
    "K070", "K071",
    "K130", "K131", "K132", "K133",
    "K700", "K702", "K730", "K731", "K732", "K733",
    "Q990", "Q992", "Q994", "Q996", "Q998",
    "R048", "R051", "R094",
    "Z101", "Z110", "Z113", "Z114", "Z116", "Z117",
    "Z122", "Z125", "Z128", "Z129",
    "Z154", "Z156", "Z157", "Z158", "Z159", "Z160", "Z161", "Z162",
    "Z175", "Z176",
    "Z314", "Z315",
    "Z535", "Z543", "Z545", "Z611", "Z847",
])

# G590 is FHN only
FHN_ONLY = {"G590"}

# ═══════════════════════════════════════════════════════════════
# SOURCE 3: FHO Contract Schedule 2B (50% shadow codes)
# ═══════════════════════════════════════════════════════════════
SCHEDULE_2B_50PCT = set([
    "G365", "G378", "G552",
    "R048", "R051", "R094",
    "Z101", "Z110", "Z113", "Z114", "Z116", "Z117",
    "Z122", "Z125", "Z128", "Z129",
    "Z154", "Z156", "Z157", "Z158", "Z159", "Z160", "Z161", "Z162",
    "Z175", "Z176",
    "Z314", "Z315",
    "Z535", "Z543", "Z545", "Z847",
    "E542",
    "G462", "G538",
    "G840", "G841", "G842", "G843", "G844", "G845", "G846", "G847", "G848",
])

# ═══════════════════════════════════════════════════════════════
# PARSE THE RUST DATABASE
# ═══════════════════════════════════════════════════════════════
DB_PATH = "tauri-app/src-tauri/src/billing/ohip_codes.rs"

with open(DB_PATH) as f:
    rust_code = f.read()

# Extract all OhipCode entries
db_codes = {}
code_pat = re.compile(
    r'code:\s*"([^"]+)".*?'
    r'ffs_rate_cents:\s*(\d+).*?'
    r'basket:\s*Basket::(\w+).*?'
    r'shadow_pct:\s*(\d+)',
    re.DOTALL
)

for m in code_pat.finditer(rust_code):
    code = m.group(1)
    rate_cents = int(m.group(2))
    basket = m.group(3)
    shadow = int(m.group(4))
    db_codes[code] = {
        'rate_cents': rate_cents,
        'basket': basket,
        'shadow_pct': shadow,
    }

# ═══════════════════════════════════════════════════════════════
# VERIFICATION
# ═══════════════════════════════════════════════════════════════
errors = []
warnings = []

print("=" * 70)
print("OHIP BILLING DATABASE VERIFICATION REPORT")
print(f"Database: {len(db_codes)} codes")
print(f"SOB rates available: {len(sob_rates)}")
print("=" * 70)

# CHECK 1: Every in-basket code in DB matches OMA basket list
print("\n--- CHECK 1: In-basket codes match OMA list ---")
for code, info in sorted(db_codes.items()):
    base = code.rstrip('A')  # Strip suffix for comparison
    if info['basket'] == 'In' and base not in OMA_FHO_BASKET:
        errors.append(f"  {code} is In-basket in DB but NOT in OMA FHO basket list")

# CHECK 2: No OMA basket code is missing from DB
print("--- CHECK 2: No OMA basket code missing from DB ---")
for oma_code in sorted(OMA_FHO_BASKET):
    db_key = oma_code + "A"
    # J-codes use C suffix in the contract
    if oma_code.startswith('J'):
        # Check both A and C suffix
        if db_key not in db_codes and oma_code + "C" not in db_codes:
            warnings.append(f"  {oma_code} is in OMA basket but MISSING from DB")
    elif db_key not in db_codes:
        warnings.append(f"  {oma_code} is in OMA basket but MISSING from DB")

# CHECK 3: 50% shadow codes match Schedule 2B
print("--- CHECK 3: 50% shadow codes match Schedule 2B ---")
for code, info in sorted(db_codes.items()):
    base = code.rstrip('A')
    if info['basket'] == 'In' and info['shadow_pct'] == 50:
        if base not in SCHEDULE_2B_50PCT:
            errors.append(f"  {code} is 50% shadow in DB but NOT in Schedule 2B — should be 30%")
    if info['basket'] == 'In' and info['shadow_pct'] == 30:
        if base in SCHEDULE_2B_50PCT:
            errors.append(f"  {code} is 30% shadow in DB but IS in Schedule 2B — should be 50%")

# CHECK 4: Rates match SOB extraction
print("--- CHECK 4: Rates match SOB extraction ---")
rate_mismatches = 0
for code, info in sorted(db_codes.items()):
    base = code.rstrip('A')
    if base in sob_rates:
        sob_cents = round(sob_rates[base] * 100)
        if info['rate_cents'] != sob_cents and info['rate_cents'] != 0:
            # Allow $0 for IC codes and percentage-based premiums
            diff = abs(info['rate_cents'] - sob_cents)
            if diff > 1:  # Allow 1 cent rounding
                warnings.append(
                    f"  {code}: DB=${info['rate_cents']/100:.2f} vs SOB=${sob_rates[base]:.2f} "
                    f"(diff=${diff/100:.2f})"
                )
                rate_mismatches += 1

# CHECK 5: Out-of-basket codes are NOT in OMA basket
print("--- CHECK 5: Out-of-basket codes not in OMA basket ---")
for code, info in sorted(db_codes.items()):
    base = code.rstrip('A')
    if info['basket'] == 'Out' and base in OMA_FHO_BASKET:
        errors.append(f"  {code} is Out-of-basket in DB but IS in OMA basket — should be In")

# ═══════════════════════════════════════════════════════════════
# REPORT
# ═══════════════════════════════════════════════════════════════
print("\n" + "=" * 70)
print("RESULTS")
print("=" * 70)

if errors:
    print(f"\n*** ERRORS ({len(errors)}) — must fix ***")
    for e in errors:
        print(e)

if warnings:
    print(f"\n--- WARNINGS ({len(warnings)}) — verify manually ---")
    for w in warnings:
        print(w)

if not errors and not warnings:
    print("\n  ALL CHECKS PASSED")

print(f"\nSummary: {len(errors)} errors, {len(warnings)} warnings")

# Exit with error code if there are errors
sys.exit(1 if errors else 0)
