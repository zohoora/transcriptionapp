# OHIP Billing Code Update Guide

How to update the AMI Assist billing code database when the Ontario Schedule of Benefits changes.

**Last updated:** April 10, 2026
**Current SOB:** March 27, 2026 (effective April 1, 2026) — `docs/billing/references/moh-schedule-benefit-2026-03-27.pdf`
**Current database:** 234 codes (145 in-basket + 89 out-of-basket) + 562 diagnostic codes

---

## When to Update

The Schedule of Benefits typically changes:
- **April 1 each year** — PSA fee increases take effect
- **Mid-year** — occasional additions/deletions via InfoBulletins

Check for new versions at: https://www.ontario.ca/page/ohip-schedule-benefits-and-fees

---

## Source Documents (Ground Truth)

**Never use LLM-generated data for rates, descriptions, or code classifications.** Always extract from these documents:

| Document | What it provides | Location |
|----------|-----------------|----------|
| **Schedule of Benefits PDF (current: March 27, 2026)** | Fee rates and descriptions for every OHIP code | `docs/billing/references/moh-schedule-benefit-2026-03-27.pdf` |
| **FHO Contract (Appendix I)** | Schedule 2: core service codes, Schedule 2B: 50% shadow codes, Schedule 3: hospital codes | `docs/billing/references/fho-contract.pdf` |
| **OMA FHO+ Hourly Rate Reference Guide** | Q310-Q313 rates and cap rules | `docs/billing/references/fho-hourly-rate-reference-guide.pdf` |
| **2026 PPC (Primary Care Physicians)** | FHO+ compensation summary | `docs/billing/references/2026 PPC.pdf` |
| **Physician's Guide to Uninsured Services** | What OHIP does NOT cover (reference only — not billed through this app) | `docs/billing/references/physicians-guide-to-uninsured-services.pdf` |
| **Schedule of Fees — Uninsured Services** | Suggested fees for uninsured services (reference only) | `docs/billing/references/schedule-of-fees-suggested-uninsured.pdf` |

All reference documents are version-controlled in the repo under `docs/billing/references/`. When a new SOB or FHO contract is released, drop the new PDF into that directory, update the filename constants in `scripts/audit_ohip_codes.py` (`SOB_PDF`, `FHO_PDF`), and re-run the audit.

Check ontario.ca for new SOB versions: https://www.ontario.ca/page/ohip-schedule-benefits-and-fees

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
| `docs/billing/references/moh-schedule-benefit-2026-03-27.pdf` | Current Schedule of Benefits (authoritative source for fees and descriptions) |
| `docs/billing/references/fho-contract.pdf` | FHO contract with Schedules 2, 2A, 2B, 3 |
| `docs/billing/references/fho-hourly-rate-reference-guide.pdf` | Q310-Q313 time-based billing details |
| `docs/billing/references/2026 PPC.pdf` | Primary Care Physicians compensation summary (FHO+ context) |

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

## Lessons Learned

### Codes with multiple descriptions in the SOB

**A single OHIP code can appear more than once in the SOB with genuinely different descriptions.** This happens when one code covers multiple billable scenarios that share a fee.

**Classic example — G372** (page J54, both in the 2025 and March 2026 SOB):

```
G372 - with visit (each injection) ...... $4.55
G373 - sole reason (first injection) .... $7.90
G372 - each additional injection ........ $4.55
```

G372 has two valid meanings at the same fee:

1. **Primary:** billed alongside an assessment code (e.g., A007A) — one G372A per injection performed during the visit.
2. **Secondary:** billed alongside G373A (sole-reason scenario) as "each additional" injection beyond the first.

Earlier versions of our database stored G372A with the secondary description only (`"Each Additional Injection (with visit)"`), which mashed up both meanings incorrectly and confused clinicians reading the billing tab — they would ask "where's the first injection code?" when in reality G372A *was* the first injection code when billed with a visit.

The fix: our G372A description now matches SOB primary wording verbatim (`"IM/SC/Intradermal Injection — with visit (each injection)"`), with a Rust code comment documenting the dual meaning. The secondary "each additional" meaning is still handled correctly by the `ADDON_CODE_PAIRS['G373A'] → G372A` mapping in `billingUtils.ts` — clicking `+` on G373A adds G372A, matching the sole-reason multi-injection scenario.

### Automatic detection via the audit script

`scripts/audit_ohip_codes.py` detects this class of issue automatically. The `extract_sob_codes()` function now tracks **all** occurrences of every code in the SOB (not just the last-seen or highest-fee match), and any code with 2+ genuinely distinct description rows is reported in the audit under **Section 6: Codes With Multiple Descriptions In SOB**.

When a new SOB is released:

```bash
python3 scripts/audit_ohip_codes.py
# Review Section 6 in /tmp/ohip_audit_report.txt
# For each flagged code:
#   1. Confirm the DB description captures the primary/dominant meaning
#   2. Add a Rust code comment documenting the secondary meaning(s)
#   3. Verify the ADDON_CODE_PAIRS logic (if applicable) handles the secondary case
```

This is the tripwire that would have caught the G372 description bug before it shipped.

---

## Key Principles

1. **Never trust LLM-generated rates or descriptions.** Always extract from the actual PDF documents.
2. **Use the scripts.** They exist to eliminate human/LLM error in the extraction pipeline.
3. **Run verification before committing.** The `verify_ohip_codes.py` script must pass with 0 errors.
4. **The FHO contract is authoritative for basket/shadow classification.** The SOB is authoritative for rates and descriptions. The OMA basket list is a convenience summary of the contract.
5. **Keep the scripts updated.** When the source document formats change, update the extraction scripts rather than doing manual work.
6. **Review Section 6 of the audit report** for codes with multiple descriptions. A single "latest match wins" extraction silently loses the other meaning.
