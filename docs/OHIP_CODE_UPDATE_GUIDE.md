# OHIP Billing Code Update Guide

How to update the AMI Assist billing code database when the Ontario Schedule of Benefits changes.

**Last updated:** April 7, 2026
**Current SOB:** March 9, 2026 (effective April 1, 2026)
**Current database:** 206 codes (145 in-basket + 61 out-of-basket)

---

## When to Update

The Schedule of Benefits typically changes:
- **April 1 each year** — PSA fee increases take effect
- **Mid-year** — occasional additions/deletions via InfoBulletins

Check for new versions at: https://www.ontario.ca/page/ohip-schedule-benefits-and-fees

---

## Source Documents (Ground Truth)

**Never use LLM-generated data for rates, descriptions, or code classifications.** Always extract from these documents:

| Document | What it provides | Where to get it |
|----------|-----------------|-----------------|
| **Schedule of Benefits PDF** | Fee rates and descriptions for every OHIP code | ontario.ca/page/ohip-schedule-benefits-and-fees |
| **OMA "Fee Codes in FHO and FHN Basket"** | Which codes are in-basket (capitation + shadow billing) | oma.org member resources (PDF) |
| **FHO Contract Appendix I** | Schedule 2: core service codes, Schedule 2B: 50% shadow codes, Schedule 3: hospital codes | `/Users/backoffice/oma/fho-contract.pdf` |
| **OMA FHO+ Hourly Rate Reference Guide** | Q310-Q313 rates and cap rules | `/Users/backoffice/oma/fho-hourly-rate-reference-guide.pdf` |

Local copies of OMA documents are in `/Users/backoffice/oma/`.

---

## Step-by-Step Update Process

### Step 1: Download the new Schedule of Benefits PDF

```bash
# Download from ontario.ca (URL changes with each release)
# Example for April 2026:
curl -o /tmp/sob_2026.pdf "https://www.ontario.ca/files/2026-03/moh-schedule-benefit-2026-03-27.pdf"
```

### Step 2: Extract text from the PDF

```bash
# pdftotext must be installed (brew install poppler)
pdftotext -layout /tmp/sob_2026.pdf /tmp/sob_2026.txt

# Verify extraction worked
wc -l /tmp/sob_2026.txt
# Should be ~30,000-40,000 lines
```

### Step 3: Extract fee rates programmatically

```bash
cd /Users/backoffice/transcriptionapp

# This script parses the SOB text and extracts code → fee mappings
python3 scripts/extract_sob_fees.py /tmp/sob_2026.txt

# Output:
# - Prints found/missing codes to terminal
# - Writes /tmp/sob_fees.json (rates only)
# - Writes /tmp/sob_fees_final.json (rates + manual additions)
```

**Review the output.** The script will report missing codes — these need manual lookup in the SOB text:

```bash
# Search for a specific code in the SOB text
grep "CODE_NUMBER" /tmp/sob_2026.txt | head -10

# Example: find B990 rate
grep "B990" /tmp/sob_2026.txt | grep -oE '[0-9]+\.[0-9]{2}' | head -1
```

For codes not found by the script, manually add them to `/tmp/sob_fees_final.json`.

### Step 4: Generate the Rust code

```bash
# This script reads the extracted fees + OMA basket list
# and generates Rust OhipCode entries
python3 scripts/generate_ohip_codes.py > /tmp/generated_ohip_codes.rs

# Review the output
head -20 /tmp/generated_ohip_codes.rs
tail -5 /tmp/generated_ohip_codes.rs
```

### Step 5: Replace the database

Open `tauri-app/src-tauri/src/billing/ohip_codes.rs` and replace the contents of the `OHIP_CODES` array with the generated code.

**Keep these sections unchanged:**
- `OhipCode` struct definition
- `Basket` and `CodeCategory` enums
- `ExclusionGroup` struct and `EXCLUSION_GROUPS` array
- `CODE_MAP` LazyLock and lookup functions
- All `#[cfg(test)]` tests

**Update:**
- The `OHIP_CODES` array contents
- The `test_code_count` assertion to match the new total
- Any test assertions that check specific rates

### Step 6: Update the FHO contract schedules (if changed)

If a new FHO contract is issued:

1. **Schedule 2** (pages 75-82): Core services in the base. Update the `OMA_FHO_BASKET` set in `generate_ohip_codes.py` and `verify_ohip_codes.py`.

2. **Schedule 2B** (pages 87-89): Procedures with 50% shadow billing. Update the `SCHEDULE_2B_50PCT` set in `generate_ohip_codes.py` and `verify_ohip_codes.py`.

3. **Schedule 2A** (pages 83-86): Long-term care base rate codes. These are W-prefix codes. Add to the database if the practice does LTC work.

4. **Schedule 3** (pages 90-91): Hospital service codes. These are out-of-basket at 100% FFS.

### Step 7: Run the verification script

```bash
python3 scripts/verify_ohip_codes.py
```

This checks:
1. Every in-basket code in the DB matches the OMA FHO basket list
2. No OMA basket code is missing from the DB
3. Every 50% shadow code matches Schedule 2B
4. Rates match the SOB extraction (within $0.01 tolerance)
5. No out-of-basket code is incorrectly in the OMA basket

**The script must report 0 errors before committing.**

### Step 8: Update frontend tooltips

The file `tauri-app/src/components/billing/billingUtils.ts` contains `OHIP_CODE_CRITERIA` — a lookup table mapping each code to a tooltip description shown when physicians hover over codes in the billing tab.

Update this to match any new/changed code descriptions.

### Step 9: Update exclusion groups (if rules changed)

Exclusion groups are defined in two places (must stay in sync):
- **Rust:** `EXCLUSION_GROUPS` in `tauri-app/src-tauri/src/billing/ohip_codes.rs`
- **TypeScript:** `EXCLUSION_GROUPS` in `tauri-app/src/components/billing/billingUtils.ts`

### Step 10: Compile and test

```bash
# Rust
cd tauri-app/src-tauri
cargo check
cargo test billing

# TypeScript
cd tauri-app
npx tsc --noEmit

# Frontend tests
pnpm test:run

# Full verification
cd /Users/backoffice/transcriptionapp
python3 scripts/verify_ohip_codes.py
```

### Step 11: Commit

```bash
git add -A
git commit -m "feat: update OHIP rates to [DATE] Schedule of Benefits"
git push origin main
```

---

## File Map

| File | Purpose |
|------|---------|
| `scripts/extract_sob_fees.py` | Extracts fee rates from SOB PDF text |
| `scripts/generate_ohip_codes.py` | Generates Rust code from extracted data |
| `scripts/verify_ohip_codes.py` | Verifies database against all source documents |
| `tauri-app/src-tauri/src/billing/ohip_codes.rs` | The OHIP code database (Rust) |
| `tauri-app/src/components/billing/billingUtils.ts` | Frontend tooltips + exclusion groups (TypeScript) |
| `/Users/backoffice/oma/fho-contract.pdf` | FHO contract with Schedules 2, 2A, 2B, 3 |
| `/Users/backoffice/oma/fho-hourly-rate-reference-guide.pdf` | Q310-Q313 time-based billing details |

---

## Common Issues

### Script doesn't find a code's rate
The SOB PDF has inconsistent formatting. Search manually:
```bash
grep "CODE_NUMBER" /tmp/sob_2026.txt | head -20
```
Look for the fee amount near the code. Add it to `/tmp/sob_fees_final.json` manually.

### Code in OMA basket but not in SOB
Some codes (like C903, J301C suffix variants) appear in the OMA basket list but use different suffixes in the SOB. Check if the code exists with a different suffix (A vs C vs B).

### Spirometry codes: A suffix vs C suffix
The FHO contract lists spirometry as J301C, J304C, J324C, J327C (professional component). The SOB may list them differently. The database uses A suffix for all codes — this is the standard FFS suffix.

### Rate is $0.00 for a code
Some codes are percentage-based (Q012A after-hours premium), administrative (Q200A rostering fee), or individually considered (R051A, E082A). These legitimately have $0 rates in the database.

### Shadow rate disagreement
If a code appears in Schedule 2B → it gets 50% shadow.
If a code is in the OMA basket but NOT in Schedule 2B → it gets 30% shadow.
If a code is NOT in the OMA basket → it's out-of-basket at 100% FFS.
The `verify_ohip_codes.py` script catches misclassifications.

---

## Key Principles

1. **Never trust LLM-generated rates or descriptions.** Always extract from the actual PDF documents.
2. **Use the scripts.** They exist to eliminate human/LLM error in the extraction pipeline.
3. **Run verification before committing.** The `verify_ohip_codes.py` script must pass with 0 errors.
4. **The FHO contract is authoritative for basket/shadow classification.** The SOB is authoritative for rates and descriptions. The OMA basket list is a convenience summary of the contract.
5. **Keep the scripts updated.** When the source document formats change, update the extraction scripts rather than doing manual work.
