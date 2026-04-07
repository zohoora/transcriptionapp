#!/usr/bin/env python3
"""
Generate ohip_codes.rs OHIP_CODES array entries from SOB-extracted data.

Uses /tmp/sob_fees_final.json (rates) and /tmp/sob_descriptions.json (descriptions)
as ground truth, cross-referenced with the OMA FHO basket list.

Output: Rust code for each OhipCode entry that can be pasted into ohip_codes.rs
"""

import json
import math

# Load SOB data
with open('/tmp/sob_fees_final.json') as f:
    fees = json.load(f)

with open('/tmp/sob_descriptions.json') as f:
    descriptions = json.load(f)

# ══════════════════════════════════════════════════════════════════════
# OMA FHO BASKET LIST (from the authoritative OMA PDF, December 2023)
# Every code listed here is IN-BASKET for FHO
# ══════════════════════════════════════════════════════════════════════
FHO_BASKET = set([
    # Assessments
    "A001", "A003", "A004", "A007", "A008",
    "A101", "A102", "A110", "A112", "A777", "A900",
    # FPA codes
    "A917", "A927", "A937", "A947", "A957", "A967",
    # SVP Office
    "A990", "A994", "A996", "A998",
    # SVP Home (FHO only)
    "B990", "B992", "B993", "B994", "B996",
    # Hospital/Pre-op (FHO only)
    "C882", "C903",
    # Tray fee
    "E542",
    # Lab
    "G001", "G002", "G004", "G005", "G009", "G010", "G011", "G012", "G014",
    # Allergy
    "G123", "G197", "G202", "G205", "G209", "G212",
    # Nerve blocks
    "G223", "G227", "G228", "G231", "G235",
    # Cardiovascular
    "G271",
    # ECG
    "G310", "G313",
    # Gynaecology
    "G365", "G378", "G394", "G552",
    # Injections/Infusions
    "G370", "G371", "G372", "G373", "G375", "G377", "G379", "G381",
    "G384", "G385",
    # Other D&T
    "G420", "G435", "G462",
    # Venipuncture
    "G481", "G482", "G489",
    # Audiometry
    "G525",
    # Immunizations
    "G538", "G840", "G841", "G842", "G843", "G844", "G845", "G846", "G847", "G848",
    # Spirometry
    "J301", "J304", "J324", "J327",
    # Counselling/Mental Health
    "K001", "K002", "K003", "K004", "K005", "K006", "K007", "K008",
    "K013", "K015", "K017",
    # Home care
    "K070", "K071",
    # Periodic health
    "K130", "K131", "K132", "K133",
    # Case conference/phone
    "K700", "K702", "K730", "K731", "K732", "K733",
    # SVP Other
    "Q990", "Q992", "Q994", "Q996", "Q998",
    # Integumentary
    "R048", "R051", "R094",
    "Z101", "Z110", "Z113", "Z114", "Z116", "Z117",
    "Z122", "Z125", "Z128", "Z129",
    "Z154", "Z156", "Z157", "Z158", "Z159", "Z160", "Z161", "Z162",
    "Z175", "Z176",
    "Z314", "Z315",
    # GI/Urological/Eye
    "Z535", "Z543", "Z545", "Z611", "Z847",
])

# G590 is FHN ONLY, not FHO
FHN_ONLY = {"G590"}

# ══════════════════════════════════════════════════════════════════════
# SHADOW RATES: 50% for procedures/injections/surgery/immunizations
# 30% for assessments/counselling/visits/lab/screening
# ══════════════════════════════════════════════════════════════════════
SHADOW_50_CODES = set([
    # Procedures
    "G123", "G197", "G202", "G205", "G209", "G212",
    "G223", "G227", "G228", "G231", "G235",
    "G310", "G313",
    "G365", "G370", "G371", "G372", "G373", "G375", "G377", "G378", "G379",
    "G381", "G384", "G385", "G394",
    "G420", "G462", "G552",
    # Immunizations
    "G538", "G840", "G841", "G842", "G843", "G844", "G845", "G846", "G847", "G848",
    # Spirometry
    "J301", "J304", "J324", "J327",
    # Surgery
    "R048", "R051", "R094",
    "Z101", "Z110", "Z113", "Z114", "Z116", "Z117",
    "Z122", "Z125", "Z128", "Z129",
    "Z154", "Z156", "Z157", "Z158", "Z159", "Z160", "Z161", "Z162",
    "Z175", "Z176",
    "Z314", "Z315",
    "Z535", "Z543", "Z545", "Z611", "Z847",
    "E542",
])

# ══════════════════════════════════════════════════════════════════════
# CATEGORIES
# ══════════════════════════════════════════════════════════════════════
def get_category(code):
    if code.startswith(('A9', 'B9', 'Q99')):
        return "Premium"  # SVP premiums
    if code.startswith('A') or code.startswith(('C', 'H', 'P')):
        return "Assessment"
    if code.startswith('K'):
        return "Counselling"
    if code.startswith(('G84', 'G59', 'G53', 'G46')):
        return "Immunization"
    if code.startswith(('G0', 'G48', 'G52', 'G43', 'E4', 'E5')):
        return "Screening"
    if code.startswith(('G', 'J', 'R', 'Z')):
        return "Procedure"
    if code.startswith('E'):
        return "Screening"
    if code.startswith('Q') and code in ('Q310', 'Q311', 'Q312', 'Q313'):
        return "TimeBased"
    if code.startswith('Q'):
        return "Premium"
    return "Assessment"

# ══════════════════════════════════════════════════════════════════════
# CLEAN DESCRIPTIONS (from SOB, with manual overrides for truncated ones)
# ══════════════════════════════════════════════════════════════════════
DESC_OVERRIDES = {
    "A001": "Minor Assessment",
    "A003": "General Assessment",
    "A004": "General Re-Assessment",
    "A007": "Intermediate Assessment or Well Baby Care",
    "A008": "Mini Assessment",
    "A101": "Limited Virtual Care — Video",
    "A102": "Limited Virtual Care — Telephone",
    "A110": "Periodic Oculo-Visual Assessment (19 and below)",
    "A112": "Periodic Oculo-Visual Assessment (65 and above)",
    "A777": "Intermediate Assessment — Pronouncement of Death",
    "A888": "Emergency Department Equivalent — Partial Assessment",
    "A900": "Complex House Call Assessment",
    "A902": "House Call — Pronouncement of Death",
    "A905": "Limited Consultation",
    "A005": "Consultation",
    "A006": "Repeat Consultation",
    "A917": "FPA — Sport Medicine",
    "A927": "FPA — Allergy",
    "A937": "FPA — Pain Management",
    "A947": "FPA — Sleep Medicine",
    "A957": "FPA — Addiction Medicine",
    "A967": "FPA — Care of the Elderly",
    "A990": "SVP Office — Weekday Daytime (07:00-17:00)",
    "A994": "SVP Office — Evening (17:00-24:00) Mon-Fri",
    "A996": "SVP Office — Night (00:00-07:00)",
    "A998": "SVP Office — Sat/Sun/Holiday (07:00-24:00)",
    "B990": "SVP Home — Weekday Non-elective/Elective",
    "B992": "SVP Home — Weekday with Sacrifice of Office Hours",
    "B993": "SVP Home — Sat/Sun/Holiday (07:00-24:00)",
    "B994": "SVP Home — Evening (17:00-24:00) Mon-Fri",
    "B996": "SVP Home — Night (00:00-07:00)",
    "B998": "SVP Home — Palliative Care Visit",
    "C002": "Hospital Subsequent Visit — First Five Weeks",
    "C003": "Hospital Admission Assessment",
    "C004": "Hospital General Re-Assessment",
    "C009": "Hospital Subsequent Visit — After Thirteenth Week",
    "C010": "Hospital Supportive Care",
    "C012": "Hospital Discharge Day Management",
    "C882": "Hospital Palliative Care — MRP Subsequent Visit",
    "C903": "Pre-Dental/Pre-Operative General Assessment",
    "E079": "Smoking Cessation — Initial Discussion (add-on)",
    "E430": "Pap Tray Fee (with G365)",
    "E431": "Pap Tray Fee — Immunocompromised",
    "E542": "Tray Fee — Procedure Outside Hospital",
    "G001": "Lab — Cholesterol, Total",
    "G002": "Lab — Glucose, Quantitative/Semi-Quantitative",
    "G004": "Lab — Occult Blood",
    "G005": "Lab — Pregnancy Test",
    "G009": "Lab — Urinalysis, Routine (includes microscopy)",
    "G010": "Lab — Urinalysis Without Microscopy",
    "G011": "Lab — Fungus Culture incl KOH Prep",
    "G012": "Lab — Wet Preparation (fungus, trichomonas)",
    "G014": "Lab — Rapid Streptococcal Test",
    "G123": "Nerve Block — Obturator, Each Additional (max 4)",
    "G197": "Allergy Skin Testing — Professional Component (max 50/yr)",
    "G202": "Allergy — Hyposensitisation, Each Injection",
    "G205": "Allergy — Insect Venom Desensitisation (max 5/day)",
    "G209": "Allergy Skin Testing — Technical Component (max 50/yr)",
    "G212": "Allergy — Hyposensitisation, Sole Reason for Visit",
    "G223": "Nerve Block — Somatic/Peripheral, Additional Sites",
    "G227": "Nerve Block — Other Cranial Nerve",
    "G228": "Nerve Block — Paravertebral (cervical/thoracic/lumbar/sacral)",
    "G231": "Nerve Block — Somatic/Peripheral, One Nerve or Site",
    "G235": "Nerve Block — Supraorbital",
    "G271": "Anticoagulant Supervision — Long-Term, Phone/Month",
    "G310": "ECG Twelve Lead — Technical Component",
    "G313": "ECG Twelve Lead — Professional Component (written interp)",
    "G365": "Papanicolaou Smear — Periodic",
    "G370": "Injection/Aspiration of Joint, Bursa, Ganglion, Tendon Sheath",
    "G371": "Additional Joint/Bursa/Ganglion/Tendon Sheath (max 5)",
    "G372": "IM/SC/Intradermal — Each Additional Injection (with visit)",
    "G373": "IM/SC/Intradermal — Sole Reason (first injection)",
    "G375": "Intralesional Infiltration — 1 or 2 Lesions",
    "G377": "Intralesional Infiltration — 3 or More Lesions",
    "G378": "IUD Insertion",
    "G379": "Intravenous — Child, Adolescent or Adult",
    "G381": "Chemotherapy — Standard Agents, Minor Toxicity",
    "G384": "Trigger Point Injection — Infiltration of Tissue",
    "G385": "Trigger Point — Each Additional Site (max 2)",
    "G394": "Papanicolaou Smear — Additional/Repeat",
    "G420": "Ear Syringing/Curetting — Unilateral or Bilateral",
    "G435": "Tonometry",
    "G462": "Administration of Oral Polio Vaccine",
    "G481": "Haemoglobin Screen and/or Haematocrit",
    "G482": "Venipuncture — Child",
    "G489": "Venipuncture — Adolescent or Adult",
    "G525": "Pure Tone Audiometry — Professional Component",
    "G538": "Immunization — Other Agents",
    "G552": "IUD Removal",
    "G590": "Immunization — Influenza (FHN only, not FHO basket)",
    "G840": "Immunization — DTaP/IPV (paediatric)",
    "G841": "Immunization — DTaP-IPV-Hib (paediatric)",
    "G842": "Immunization — Hepatitis B",
    "G843": "Immunization — HPV",
    "G844": "Immunization — Meningococcal C Conjugate",
    "G845": "Immunization — MMR",
    "G846": "Immunization — Pneumococcal Conjugate",
    "G847": "Immunization — Tdap (adult)",
    "G848": "Immunization — Varicella",
    "J301": "Spirometry — Simple (VC, FEV1, FEV1/FVC)",
    "J304": "Flow Volume Loop",
    "J324": "Spirometry — Repeat After Bronchodilator",
    "J327": "Flow Volume Loop — Repeat After Bronchodilator",
    "K001": "Detention — Per Full Quarter Hour",
    "K002": "Interviews with Relatives/Authorized Decision-Maker (per unit)",
    "K003": "Interviews with CAS/Legal Guardian (per unit)",
    "K004": "Psychotherapy — Family (2+ members, per unit)",
    "K005": "Primary Mental Health Care — Individual (per unit)",
    "K006": "Hypnotherapy — Individual (per unit)",
    "K007": "Psychotherapy — Individual (per unit)",
    "K008": "Diagnostic Interview/Counselling — Child/Parent (per unit)",
    "K013": "Counselling — Individual (first 3 units K013+K040/12mo, per unit)",
    "K015": "Counselling of Relatives — Terminally Ill Patient (per unit)",
    "K017": "Periodic Health Visit — Child",
    "K022": "HIV Primary Care (per unit)",
    "K023": "Palliative Care Support (per unit)",
    "K028": "STI Management (per unit)",
    "K029": "Insulin Therapy Support (per unit)",
    "K030": "Diabetic Management Assessment",
    "K031": "Form 1 — Physician Report (Mental Health Act)",
    "K032": "Specific Neurocognitive Assessment (min 20 min)",
    "K033": "Counselling — Additional Units (per unit)",
    "K034": "Telephone Reporting — Specified Reportable Disease to MOH",
    "K035": "Mandatory Reporting — Medical Condition to Ontario MOT",
    "K036": "Northern Health Travel Grant Application Form",
    "K037": "Fibromyalgia/Myalgic Encephalomyelitis Care (per unit)",
    "K038": "Completion of LTC Health Assessment Form",
    "K039": "Smoking Cessation Follow-Up Visit",
    "K070": "Completion of Home Care Referral Form",
    "K071": "Acute Home Care Supervision (first 8 weeks)",
    "K130": "Periodic Health Visit — Adolescent",
    "K131": "Periodic Health Visit — Adult 18-64",
    "K132": "Periodic Health Visit — Adult 65+",
    "K133": "Periodic Health Visit — Adult with IDD",
    "K140": "Shared Medical Appointment — 2 Patients",
    "K141": "Shared Medical Appointment — 3 Patients",
    "K142": "Shared Medical Appointment — 4 Patients",
    "K143": "Shared Medical Appointment — 5 Patients",
    "K144": "Shared Medical Appointment — 6-12 Patients",
    "K655": "Comprehensive Geriatric Assessment (75+, annual)",
    "K656": "Geriatric Assessment — Follow-Up",
    "K700": "Palliative Care Out-Patient Case Conference (per unit)",
    "K702": "Bariatric Out-Patient Case Conference (per unit)",
    "K730": "Physician-to-Physician Phone Consultation — Referring",
    "K731": "Physician-to-Physician Phone Consultation — Consultant",
    "K732": "CritiCall Phone Consultation — Referring",
    "K733": "CritiCall Phone Consultation — Consultant",
    "K738": "Physician-to-Physician eConsult — Referring",
    "K998": "SVP Other — Sat/Sun/Holiday (07:00-24:00)",
    "P001": "Attendance at Labour and Delivery (normal)",
    "P002": "High Risk Prenatal Assessment",
    "P003": "General Assessment (Major Prenatal Visit)",
    "P004": "Minor Prenatal Assessment",
    "P005": "Antenatal Preventive Health Assessment",
    "P006": "Vaginal Delivery",
    "P007": "Postnatal Care — Hospital and/or Home",
    "P008": "Postnatal Care — Office",
    "P009": "Attendance at Labour and Delivery (complicated)",
    "P018": "Caesarean Section",
    "Q010": "Childhood Immunization Bonus",
    "Q012": "After-Hours Premium (percentage-based)",
    "Q015": "Newborn Care Episodic Fee",
    "Q040": "Diabetes Management Incentive",
    "Q042": "Smoking Cessation Counselling Fee",
    "Q050": "Heart Failure Management Incentive",
    "Q053": "HCC Complex/Vulnerable Patient Bonus",
    "Q054": "Mother and Newborn Bonus",
    "Q100": "Cervical Screening Bonus",
    "Q101": "Mammography Screening Bonus",
    "Q102": "Colorectal Cancer Screening Bonus",
    "Q200": "Per Patient Rostering Fee",
    "Q310": "Direct Patient Care — In-Person/Video/Phone (per 15 min)",
    "Q311": "Telephone Care — Not in Office (per 15 min)",
    "Q312": "Indirect Patient Care — Charting/Labs/Referrals (per 15 min)",
    "Q313": "Clinical Administration — EMR/QI/Screening (per 15 min)",
    "Q888": "Weekend Office Access Premium (FHO)",
    "R048": "Malignant Lesion Excision — Face/Neck, Single",
    "R051": "Malignant Lesion — Laser Surgery Group 1-4",
    "R094": "Malignant Lesion Excision — Other Areas, Single",
    "Z101": "Abscess/Haematoma Incision — Subcutaneous, One",
    "Z110": "Onychogryphotic Nail — Extensive Debridement",
    "Z113": "Biopsy — Any Method, Without Sutures",
    "Z114": "Foreign Body Removal — Local Anaesthetic",
    "Z116": "Biopsy — Any Method, With Sutures",
    "Z117": "Chemical/Cryotherapy Treatment — One or More Lesions",
    "Z122": "Group 3 Excision (cyst/lipoma) — Face/Neck, Single",
    "Z125": "Group 3 Excision (cyst/lipoma) — Other Areas, Single",
    "Z128": "Nail Plate Excision Requiring Anaesthesia — One",
    "Z129": "Nail Plate Excision — Multiple",
    "Z154": "Laceration Repair — Up to 5cm (face/layers)",
    "Z156": "Group 1 Excision (keratosis) — Excision & Suture, Single",
    "Z157": "Group 1 Excision (keratosis) — Excision & Suture, Two",
    "Z158": "Group 1 Excision (keratosis) — Excision & Suture, Three+",
    "Z159": "Group 1 — Electrocoagulation/Curetting, Single",
    "Z160": "Group 1 — Electrocoagulation/Curetting, Two",
    "Z161": "Group 1 — Electrocoagulation/Curetting, Three+",
    "Z162": "Group 2 (nevus) — Excision & Suture, Single",
    "Z175": "Laceration Repair — 5.1 to 10cm",
    "Z176": "Laceration Repair — Up to 5cm (simple)",
    "Z314": "Epistaxis — Cauterization, Unilateral",
    "Z315": "Epistaxis — Anterior Packing, Unilateral",
    "Z535": "Sigmoidoscopy — Rigid Scope",
    "Z543": "Anoscopy (Proctoscopy)",
    "Z545": "Thrombosed Haemorrhoid(s) Incision",
    "Z611": "Catheterization — Hospital",
    "Z847": "Corneal Foreign Body Removal — One",
}

# After-hours eligible codes (core assessments only)
AFTER_HOURS = {"A001", "A003", "A004", "A007", "A008"}

# Max per year
MAX_PER_YEAR = {
    "K029": 6,
    "K030": 4,
    "K039": 2,
}

# ══════════════════════════════════════════════════════════════════════
# GENERATE RUST CODE
# ══════════════════════════════════════════════════════════════════════

def generate_entry(code, fee, desc, basket, shadow_pct, category, after_hours, max_year):
    cents = round(fee * 100)
    ah = "true" if after_hours else "false"
    my = f"Some({max_year})" if max_year else "None"
    return f"""    OhipCode {{
        code: "{code}A",
        description: "{desc}",
        ffs_rate_cents: {cents}, // ${fee:.2f}
        basket: Basket::{basket},
        shadow_pct: {shadow_pct},
        category: CodeCategory::{category},
        after_hours_eligible: {ah},
        max_per_year: {my},
    }},"""

# Collect all codes we want in the database
all_codes = {}

# 1. All in-basket codes
for code in sorted(FHO_BASKET):
    if code in fees:
        basket = "In"
        shadow = 50 if code in SHADOW_50_CODES else 30
        cat = get_category(code)
        ah = code in AFTER_HOURS
        my = MAX_PER_YEAR.get(code)
        desc = DESC_OVERRIDES.get(code, descriptions.get(code, f"(description needed for {code})"))
        all_codes[code] = generate_entry(code, fees[code], desc, basket, shadow, cat, ah, my)

# 2. Out-of-basket codes (known GP-billable)
OUT_OF_BASKET_CODES = [
    "A005", "A006", "A888", "A905",
    "C002", "C003", "C004", "C009", "C010", "C012",
    "E079", "E430", "E431",
    "G590",
    "K022", "K023", "K028", "K029", "K030", "K031", "K032", "K033",
    "K034", "K035", "K036", "K037", "K038", "K039",
    "K140", "K141", "K142", "K143", "K144",
    "K738",
    "P001", "P002", "P003", "P004", "P005", "P006", "P007", "P008", "P009", "P018",
    "Q040", "Q042", "Q050", "Q053", "Q200", "Q888",
    "Q310", "Q311", "Q312", "Q313",
    "R048", "R051", "R094",  # These are in-basket per OMA but keeping as-is
]

for code in OUT_OF_BASKET_CODES:
    if code in fees and code not in all_codes:
        cat = get_category(code)
        ah = code in AFTER_HOURS
        my = MAX_PER_YEAR.get(code)
        desc = DESC_OVERRIDES.get(code, descriptions.get(code, f"(description needed)"))
        shadow = 100
        if code in ('Q310', 'Q311', 'Q312', 'Q313'):
            cat = "TimeBased"
        all_codes[code] = generate_entry(code, fees[code], desc, "Out", shadow, cat, ah, my)

# Print the generated Rust code
print("// ══════════════════════════════════════════════════════════════════")
print("// OHIP CODE DATABASE")
print(f"// Generated from April 2026 Schedule of Benefits (effective April 1, 2026)")
print(f"// Source: ontario.ca/files/2026-03/moh-schedule-benefit-2026-03-27.pdf")
print(f"// Basket classification: OMA Fee Codes in FHO and FHN Basket (Dec 2023)")
print(f"// Total codes: {len(all_codes)}")
print("// ══════════════════════════════════════════════════════════════════")
print()

# Group by section
in_basket = {k: v for k, v in sorted(all_codes.items()) if "Basket::In" in v}
out_basket = {k: v for k, v in sorted(all_codes.items()) if "Basket::Out" in v}

print(f"// ── IN-BASKET CODES ({len(in_basket)} codes, from OMA FHO basket list) ──")
print()
for code, entry in sorted(in_basket.items()):
    print(entry)

print()
print(f"// ── OUT-OF-BASKET CODES ({len(out_basket)} codes, 100% FFS) ──")
print()
for code, entry in sorted(out_basket.items()):
    print(entry)

print()
print(f"// Total: {len(all_codes)} codes ({len(in_basket)} in-basket + {len(out_basket)} out-of-basket)")

if __name__ == '__main__':
    main() if 'main' in dir() else None
