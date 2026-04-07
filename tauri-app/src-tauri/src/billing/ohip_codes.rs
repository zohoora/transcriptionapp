use std::collections::HashMap;
use std::sync::LazyLock;

// ── Classification enums ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Basket {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodeCategory {
    Assessment,
    Counselling,
    Procedure,
    ChronicDisease,
    Screening,
    Premium,
    TimeBased,
    Immunization,
}

// ── Static OHIP code definition ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OhipCode {
    pub code: &'static str,
    pub description: &'static str,
    pub ffs_rate_cents: u32,
    pub basket: Basket,
    pub shadow_pct: u8,
    pub category: CodeCategory,
    pub after_hours_eligible: bool,
    pub max_per_year: Option<u8>,
}

// ── Complete OHIP code database ────────────────────────────────────────────
//
// Generated from April 2026 Schedule of Benefits (effective April 1, 2026)
// Source: ontario.ca/files/2026-03/moh-schedule-benefit-2026-03-27.pdf
// Basket classification: OMA Fee Codes in FHO and FHN Basket (Dec 2023)
// Total codes: 198 (137 in-basket + 61 out-of-basket)

pub static OHIP_CODES: &[OhipCode] = &[
    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 1: IN-BASKET CODES (137 codes, from OMA FHO basket list)
    // ═══════════════════════════════════════════════════════════════════════

    // ── Assessments (in-basket, 30% shadow, Assessment) ──────────────────
    OhipCode {
        code: "A001A",
        description: "Minor Assessment",
        ffs_rate_cents: 2680, // $26.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A003A",
        description: "General Assessment",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A004A",
        description: "General Re-Assessment",
        ffs_rate_cents: 3935, // $39.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A007A",
        description: "Intermediate Assessment or Well Baby Care",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A008A",
        description: "Mini Assessment",
        ffs_rate_cents: 1340, // $13.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A101A",
        description: "Limited Virtual Care \u{2014} Video",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A102A",
        description: "Limited Virtual Care \u{2014} Telephone",
        ffs_rate_cents: 1500, // $15.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A110A",
        description: "Periodic Oculo-Visual Assessment (19 and below)",
        ffs_rate_cents: 4890, // $48.90
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A112A",
        description: "Periodic Oculo-Visual Assessment (65 and above)",
        ffs_rate_cents: 4890, // $48.90
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A777A",
        description: "Intermediate Assessment \u{2014} Pronouncement of Death",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A900A",
        description: "Complex House Call Assessment",
        ffs_rate_cents: 6480, // $64.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Focused Practice Assessments (in-basket, 30% shadow, Premium) ────
    OhipCode {
        code: "A917A",
        description: "FPA \u{2014} Sport Medicine",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A927A",
        description: "FPA \u{2014} Allergy",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A937A",
        description: "FPA \u{2014} Pain Management",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A947A",
        description: "FPA \u{2014} Sleep Medicine",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A957A",
        description: "FPA \u{2014} Addiction Medicine",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A967A",
        description: "FPA \u{2014} Care of the Elderly",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Physician Office (in-basket, 30% shadow, Premium) ──────────
    OhipCode {
        code: "A990A",
        description: "SVP Office \u{2014} Weekday Daytime (07:00-17:00)",
        ffs_rate_cents: 2055, // $20.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A994A",
        description: "SVP Office \u{2014} Evening (17:00-24:00) Mon-Fri",
        ffs_rate_cents: 6170, // $61.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A996A",
        description: "SVP Office \u{2014} Night (00:00-07:00)",
        ffs_rate_cents: 10280, // $102.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A998A",
        description: "SVP Office \u{2014} Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 7710, // $77.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Patient's Home (in-basket, 30% shadow, Premium) ────────────
    OhipCode {
        code: "B990A",
        description: "SVP Home \u{2014} Weekday Non-elective/Elective",
        ffs_rate_cents: 2825, // $28.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B992A",
        description: "SVP Home \u{2014} Weekday with Sacrifice of Office Hours",
        ffs_rate_cents: 4525, // $45.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B993A",
        description: "SVP Home \u{2014} Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 8480, // $84.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B994A",
        description: "SVP Home \u{2014} Evening (17:00-24:00) Mon-Fri",
        ffs_rate_cents: 6785, // $67.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B996A",
        description: "SVP Home \u{2014} Night (00:00-07:00)",
        ffs_rate_cents: 11310, // $113.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Hospital (in-basket, 30% shadow, Assessment) ─────────────────────
    OhipCode {
        code: "C882A",
        description: "Hospital Palliative Care \u{2014} MRP Subsequent Visit",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    OhipCode {
        code: "C903A",
        description: "Pre-Dental/Pre-Operative General Assessment",
        ffs_rate_cents: 9560, // $95.60 (same as A003 general assessment)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Tray Fee (in-basket, 50% shadow, Screening) ─────────────────────
    OhipCode {
        code: "E542A",
        description: "Tray Fee \u{2014} Procedure Outside Hospital",
        ffs_rate_cents: 1265, // $12.65
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Lab (in-basket, 30% shadow, Screening) ──────────────────────────
    OhipCode {
        code: "G001A",
        description: "Lab \u{2014} Cholesterol, Total",
        ffs_rate_cents: 570, // $5.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G002A",
        description: "Lab \u{2014} Glucose, Quantitative/Semi-Quantitative",
        ffs_rate_cents: 226, // $2.26
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G004A",
        description: "Lab \u{2014} Occult Blood",
        ffs_rate_cents: 158, // $1.58
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G005A",
        description: "Lab \u{2014} Pregnancy Test",
        ffs_rate_cents: 388, // $3.88
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G009A",
        description: "Lab \u{2014} Urinalysis, Routine (includes microscopy)",
        ffs_rate_cents: 490, // $4.90
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G010A",
        description: "Lab \u{2014} Urinalysis Without Microscopy",
        ffs_rate_cents: 264, // $2.64
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G011A",
        description: "Lab \u{2014} Fungus Culture incl KOH Prep",
        ffs_rate_cents: 1305, // $13.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G012A",
        description: "Lab \u{2014} Wet Preparation (fungus, trichomonas)",
        ffs_rate_cents: 193, // $1.93
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G014A",
        description: "Lab \u{2014} Rapid Streptococcal Test",
        ffs_rate_cents: 680, // $6.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Nerve Blocks (in-basket, 50% shadow, Procedure) ─────────────────
    OhipCode {
        code: "G123A",
        description: "Nerve Block \u{2014} Obturator, Each Additional (max 4)",
        ffs_rate_cents: 1710, // $17.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Allergy (in-basket, 50% shadow, Procedure) ──────────────────────
    OhipCode {
        code: "G197A",
        description: "Allergy Skin Testing \u{2014} Professional Component (max 50/yr)",
        ffs_rate_cents: 39, // $0.39
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G202A",
        description: "Allergy \u{2014} Hyposensitisation, Each Injection",
        ffs_rate_cents: 715, // $7.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G205A",
        description: "Allergy \u{2014} Insect Venom Desensitisation (max 5/day)",
        ffs_rate_cents: 1315, // $13.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G209A",
        description: "Allergy Skin Testing \u{2014} Technical Component (max 50/yr)",
        ffs_rate_cents: 80, // $0.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G212A",
        description: "Allergy \u{2014} Hyposensitisation, Sole Reason for Visit",
        ffs_rate_cents: 1285, // $12.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Nerve Blocks cont. (in-basket, 50% shadow, Procedure) ───────────
    OhipCode {
        code: "G223A",
        description: "Nerve Block \u{2014} Somatic/Peripheral, Additional Sites",
        ffs_rate_cents: 1710, // $17.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G227A",
        description: "Nerve Block \u{2014} Other Cranial Nerve",
        ffs_rate_cents: 5465, // $54.65
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G228A",
        description: "Nerve Block \u{2014} Paravertebral (cervical/thoracic/lumbar/sacral)",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G231A",
        description: "Nerve Block \u{2014} Somatic/Peripheral, One Nerve or Site",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G235A",
        description: "Nerve Block \u{2014} Supraorbital",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Cardiovascular (in-basket, 30% shadow, Procedure) ───────────────
    OhipCode {
        code: "G271A",
        description: "Anticoagulant Supervision \u{2014} Long-Term, Phone/Month",
        ffs_rate_cents: 1310, // $13.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── ECG (in-basket, 50% shadow, Procedure) ─────────────────────────
    OhipCode {
        code: "G310A",
        description: "ECG Twelve Lead \u{2014} Technical Component",
        ffs_rate_cents: 770, // $7.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G313A",
        description: "ECG Twelve Lead \u{2014} Professional Component (written interp)",
        ffs_rate_cents: 455, // $4.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Gynaecology (in-basket, 50% shadow, Procedure) ─────────────────
    OhipCode {
        code: "G365A",
        description: "Papanicolaou Smear \u{2014} Periodic",
        ffs_rate_cents: 1200, // $12.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Injections/Infusions (in-basket, 50% shadow, Procedure) ─────────
    OhipCode {
        code: "G370A",
        description: "Injection/Aspiration of Joint, Bursa, Ganglion, Tendon Sheath",
        ffs_rate_cents: 2025, // $20.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G371A",
        description: "Additional Joint/Bursa/Ganglion/Tendon Sheath (max 5)",
        ffs_rate_cents: 1990, // $19.90
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G372A",
        description: "IM/SC/Intradermal \u{2014} Each Additional Injection (with visit)",
        ffs_rate_cents: 455, // $4.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G373A",
        description: "IM/SC/Intradermal \u{2014} Sole Reason (first injection)",
        ffs_rate_cents: 790, // $7.90
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G375A",
        description: "Intralesional Infiltration \u{2014} 1 or 2 Lesions",
        ffs_rate_cents: 1050, // $10.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G377A",
        description: "Intralesional Infiltration \u{2014} 3 or More Lesions",
        ffs_rate_cents: 1580, // $15.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G378A",
        description: "IUD Insertion",
        ffs_rate_cents: 4750, // $47.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G379A",
        description: "Intravenous \u{2014} Child, Adolescent or Adult",
        ffs_rate_cents: 615, // $6.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G381A",
        description: "Chemotherapy \u{2014} Standard Agents, Minor Toxicity",
        ffs_rate_cents: 5450, // $54.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G384A",
        description: "Trigger Point Injection \u{2014} Infiltration of Tissue",
        ffs_rate_cents: 885, // $8.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G385A",
        description: "Trigger Point \u{2014} Each Additional Site (max 2)",
        ffs_rate_cents: 455, // $4.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G394A",
        description: "Papanicolaou Smear \u{2014} Additional/Repeat",
        ffs_rate_cents: 1200, // $12.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Other D&T (in-basket, various shadow) ───────────────────────────
    OhipCode {
        code: "G420A",
        description: "Ear Syringing/Curetting \u{2014} Unilateral or Bilateral",
        ffs_rate_cents: 1315, // $13.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G435A",
        description: "Tonometry",
        ffs_rate_cents: 510, // $5.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G462A",
        description: "Administration of Oral Polio Vaccine",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Lab/Venipuncture (in-basket, 30% shadow, Screening) ─────────────
    OhipCode {
        code: "G481A",
        description: "Haemoglobin Screen and/or Haematocrit",
        ffs_rate_cents: 137, // $1.37
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G482A",
        description: "Venipuncture \u{2014} Child",
        ffs_rate_cents: 735, // $7.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G489A",
        description: "Venipuncture \u{2014} Adolescent or Adult",
        ffs_rate_cents: 354, // $3.54
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Audiometry (in-basket, 30% shadow, Screening) ───────────────────
    OhipCode {
        code: "G525A",
        description: "Pure Tone Audiometry \u{2014} Professional Component",
        ffs_rate_cents: 585, // $5.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Immunizations (in-basket, 50% shadow, Immunization) ─────────────
    OhipCode {
        code: "G538A",
        description: "Immunization \u{2014} Other Agents",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G552A",
        description: "IUD Removal",
        ffs_rate_cents: 2380, // $23.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G840A",
        description: "Immunization \u{2014} DTaP/IPV (paediatric)",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G841A",
        description: "Immunization \u{2014} DTaP-IPV-Hib (paediatric)",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G842A",
        description: "Immunization \u{2014} Hepatitis B",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G843A",
        description: "Immunization \u{2014} HPV",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G844A",
        description: "Immunization \u{2014} Meningococcal C Conjugate",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G845A",
        description: "Immunization \u{2014} MMR",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G846A",
        description: "Immunization \u{2014} Pneumococcal Conjugate",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G847A",
        description: "Immunization \u{2014} Tdap (adult)",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G848A",
        description: "Immunization \u{2014} Varicella",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Spirometry (in-basket, 30% shadow, Procedure) ───────────────────
    OhipCode {
        code: "J301A",
        description: "Spirometry \u{2014} Simple (VC, FEV1, FEV1/FVC)",
        ffs_rate_cents: 1085, // $10.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J304A",
        description: "Flow Volume Loop",
        ffs_rate_cents: 2155, // $21.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J324A",
        description: "Spirometry \u{2014} Repeat After Bronchodilator",
        ffs_rate_cents: 420, // $4.20
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J327A",
        description: "Flow Volume Loop \u{2014} Repeat After Bronchodilator",
        ffs_rate_cents: 765, // $7.65
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Counselling/Mental Health (in-basket, 30% shadow, Counselling) ───
    OhipCode {
        code: "K001A",
        description: "Detention \u{2014} Per Full Quarter Hour",
        ffs_rate_cents: 2110, // $21.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K002A",
        description: "Interviews with Relatives/Authorized Decision-Maker (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K003A",
        description: "Interviews with CAS/Legal Guardian (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K004A",
        description: "Psychotherapy \u{2014} Family (2+ members, per unit)",
        ffs_rate_cents: 8685, // $86.85
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K005A",
        description: "Primary Mental Health Care \u{2014} Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K006A",
        description: "Hypnotherapy \u{2014} Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K007A",
        description: "Psychotherapy \u{2014} Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K008A",
        description: "Diagnostic Interview/Counselling \u{2014} Child/Parent (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K013A",
        description: "Counselling \u{2014} Individual (first 3 units K013+K040/12mo, per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K015A",
        description: "Counselling of Relatives \u{2014} Terminally Ill Patient (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K017A",
        description: "Periodic Health Visit \u{2014} Child",
        ffs_rate_cents: 4955, // $49.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Home Care (in-basket, 30% shadow, Counselling) ──────────────────
    OhipCode {
        code: "K070A",
        description: "Completion of Home Care Referral Form",
        ffs_rate_cents: 3475, // $34.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K071A",
        description: "Acute Home Care Supervision (first 8 weeks)",
        ffs_rate_cents: 2195, // $21.95
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Periodic Health Visits (in-basket, 30% shadow, Counselling) ─────
    OhipCode {
        code: "K130A",
        description: "Periodic Health Visit \u{2014} Adolescent",
        ffs_rate_cents: 8710, // $87.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K131A",
        description: "Periodic Health Visit \u{2014} Adult 18-64",
        ffs_rate_cents: 6425, // $64.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K132A",
        description: "Periodic Health Visit \u{2014} Adult 65+",
        ffs_rate_cents: 9135, // $91.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K133A",
        description: "Periodic Health Visit \u{2014} Adult with IDD",
        ffs_rate_cents: 16425, // $164.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Case Conference/Phone Consult (in-basket, 30% shadow, Counselling)
    OhipCode {
        code: "K700A",
        description: "Palliative Care Out-Patient Case Conference (per unit)",
        ffs_rate_cents: 3705, // $37.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K702A",
        description: "Bariatric Out-Patient Case Conference (per unit)",
        ffs_rate_cents: 3705, // $37.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K730A",
        description: "Physician-to-Physician Phone Consultation \u{2014} Referring",
        ffs_rate_cents: 3705, // $37.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K731A",
        description: "Physician-to-Physician Phone Consultation \u{2014} Consultant",
        ffs_rate_cents: 4775, // $47.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K732A",
        description: "CritiCall Phone Consultation \u{2014} Referring",
        ffs_rate_cents: 3705, // $37.05
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K733A",
        description: "CritiCall Phone Consultation \u{2014} Consultant",
        ffs_rate_cents: 4775, // $47.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Integumentary Surgery (in-basket, 50% shadow, Procedure) ────────
    OhipCode {
        code: "R048A",
        description: "Malignant Lesion Excision \u{2014} Face/Neck, Single",
        ffs_rate_cents: 10095, // $100.95
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R051A",
        description: "Malignant Lesion \u{2014} Laser Surgery Group 1\u{2013}4",
        ffs_rate_cents: 0, // I.C. (individually considered — no fixed SOB rate)
        basket: Basket::In,
        shadow_pct: 50, // Schedule 2B — 50% shadow
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R094A",
        description: "Malignant Lesion Excision \u{2014} Other Areas, Single",
        ffs_rate_cents: 6370, // $63.70
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z101A",
        description: "Abscess/Haematoma Incision \u{2014} Subcutaneous, One",
        ffs_rate_cents: 2820, // $28.20
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z110A",
        description: "Onychogryphotic Nail \u{2014} Extensive Debridement",
        ffs_rate_cents: 1915, // $19.15
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z113A",
        description: "Biopsy \u{2014} Any Method, Without Sutures",
        ffs_rate_cents: 3245, // $32.45
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z114A",
        description: "Foreign Body Removal \u{2014} Local Anaesthetic",
        ffs_rate_cents: 2765, // $27.65
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z116A",
        description: "Biopsy \u{2014} Any Method, With Sutures",
        ffs_rate_cents: 3245, // $32.45
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z117A",
        description: "Chemical/Cryotherapy Treatment \u{2014} One or More Lesions",
        ffs_rate_cents: 1275, // $12.75
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z122A",
        description: "Group 3 Excision (cyst/lipoma) \u{2014} Face/Neck, Single",
        ffs_rate_cents: 4220, // $42.20
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z125A",
        description: "Group 3 Excision (cyst/lipoma) \u{2014} Other Areas, Single",
        ffs_rate_cents: 3505, // $35.05
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z128A",
        description: "Nail Plate Excision Requiring Anaesthesia \u{2014} One",
        ffs_rate_cents: 3630, // $36.30
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z129A",
        description: "Nail Plate Excision \u{2014} Multiple",
        ffs_rate_cents: 3920, // $39.20
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z154A",
        description: "Laceration Repair \u{2014} Up to 5cm (face/layers)",
        ffs_rate_cents: 3935, // $39.35
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z156A",
        description: "Group 1 Excision (keratosis) \u{2014} Excision & Suture, Single",
        ffs_rate_cents: 2190, // $21.90
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z157A",
        description: "Group 1 Excision (keratosis) \u{2014} Excision & Suture, Two",
        ffs_rate_cents: 2905, // $29.05
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z158A",
        description: "Group 1 Excision (keratosis) \u{2014} Excision & Suture, Three+",
        ffs_rate_cents: 4850, // $48.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z159A",
        description: "Group 1 \u{2014} Electrocoagulation/Curetting, Single",
        ffs_rate_cents: 1155, // $11.55
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z160A",
        description: "Group 1 \u{2014} Electrocoagulation/Curetting, Two",
        ffs_rate_cents: 1735, // $17.35
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z161A",
        description: "Group 1 \u{2014} Electrocoagulation/Curetting, Three+",
        ffs_rate_cents: 2870, // $28.70
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z162A",
        description: "Group 2 (nevus) \u{2014} Excision & Suture, Single",
        ffs_rate_cents: 2190, // $21.90
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z175A",
        description: "Laceration Repair \u{2014} 5.1 to 10cm",
        ffs_rate_cents: 3935, // $39.35
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z176A",
        description: "Laceration Repair \u{2014} Up to 5cm (simple)",
        ffs_rate_cents: 2190, // $21.90
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z314A",
        description: "Epistaxis \u{2014} Cauterization, Unilateral",
        ffs_rate_cents: 1150, // $11.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z315A",
        description: "Epistaxis \u{2014} Anterior Packing, Unilateral",
        ffs_rate_cents: 1535, // $15.35
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── GI/Urological/Eye (in-basket, 50% shadow, Procedure) ───────────
    OhipCode {
        code: "Z535A",
        description: "Sigmoidoscopy \u{2014} Rigid Scope",
        ffs_rate_cents: 3680, // $36.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z543A",
        description: "Anoscopy (Proctoscopy)",
        ffs_rate_cents: 870, // $8.70
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z545A",
        description: "Thrombosed Haemorrhoid(s) Incision",
        ffs_rate_cents: 2525, // $25.25
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z611A",
        description: "Catheterization \u{2014} Hospital",
        ffs_rate_cents: 915, // $9.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z847A",
        description: "Corneal Foreign Body Removal \u{2014} One",
        ffs_rate_cents: 3300, // $33.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Other Setting (in-basket, 30% shadow, Premium) ─────────────
    OhipCode {
        code: "Q990A",
        description: "SVP Other \u{2014} Weekday Daytime (07:00-17:00)",
        ffs_rate_cents: 2055, // $20.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q992A",
        description: "SVP Other \u{2014} Weekday with Sacrifice of Office Hours",
        ffs_rate_cents: 4110, // $41.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q994A",
        description: "SVP Other \u{2014} Evening (17:00-24:00) Mon-Fri",
        ffs_rate_cents: 6170, // $61.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q996A",
        description: "SVP Other \u{2014} Night (00:00-07:00)",
        ffs_rate_cents: 10280, // $102.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q998A",
        description: "SVP Other \u{2014} Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 7710, // $77.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 2: OUT-OF-BASKET CODES (100% FFS)
    // ═══════════════════════════════════════════════════════════════════════

    // ── Consultations (out-of-basket) ──────────────────────────────────
    OhipCode {
        code: "A005A",
        description: "Consultation",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A006A",
        description: "Repeat Consultation",
        ffs_rate_cents: 4710, // $47.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A888A",
        description: "Emergency Department Equivalent \u{2014} Partial Assessment",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A905A",
        description: "Limited Consultation",
        ffs_rate_cents: 8015, // $80.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Hospital Visits (out-of-basket) ─────────────────────────────────
    OhipCode {
        code: "C002A",
        description: "Hospital Subsequent Visit \u{2014} First Five Weeks",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C003A",
        description: "Hospital Admission Assessment",
        ffs_rate_cents: 8965, // $89.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C004A",
        description: "Hospital General Re-Assessment",
        ffs_rate_cents: 3835, // $38.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C009A",
        description: "Hospital Subsequent Visit \u{2014} After Thirteenth Week",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C010A",
        description: "Hospital Supportive Care",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C012A",
        description: "Hospital Discharge Day Management",
        ffs_rate_cents: 3170, // $31.70
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C005A",
        description: "Hospital Consultation",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C006A",
        description: "Hospital Repeat Consultation",
        ffs_rate_cents: 4710, // $47.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C121A",
        description: "Hospital Additional Visit \u{2014} Intercurrent Illness",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C124A",
        description: "Hospital Subsequent Visit by MRP \u{2014} Day of Discharge",
        ffs_rate_cents: 7180, // $71.80
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C777A",
        description: "Pronouncement of Death",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C905A",
        description: "Hospital Limited Consultation",
        ffs_rate_cents: 8125, // $81.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E082A",
        description: "Admission Assessment MRP Premium (add 30%)",
        ffs_rate_cents: 0, // Percentage add-on (30% of admission assessment), no fixed rate
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Premiums/Incentives (out-of-basket) ─────────────────────────────
    OhipCode {
        code: "E079A",
        description: "Smoking Cessation \u{2014} Initial Discussion (add-on)",
        ffs_rate_cents: 1595, // $15.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E430A",
        description: "Pap Tray Fee (with G365)",
        ffs_rate_cents: 1195, // $11.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E431A",
        description: "Pap Tray Fee \u{2014} Immunocompromised",
        ffs_rate_cents: 1195, // $11.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Influenza (FHN only, not FHO basket — out-of-basket) ────────────
    OhipCode {
        code: "G590A",
        description: "Immunization \u{2014} Influenza (FHN only, not FHO basket)",
        ffs_rate_cents: 880, // $8.80
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Chronic Disease/Counselling (out-of-basket) ─────────────────────
    OhipCode {
        code: "K022A",
        description: "HIV Primary Care (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K023A",
        description: "Palliative Care Support (per unit)",
        ffs_rate_cents: 8525, // $85.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K028A",
        description: "STI Management (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K029A",
        description: "Insulin Therapy Support (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: Some(6),
    },
    OhipCode {
        code: "K030A",
        description: "Diabetic Management Assessment",
        ffs_rate_cents: 4575, // $45.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: Some(4),
    },
    OhipCode {
        code: "K031A",
        description: "Form 1 \u{2014} Physician Report (Mental Health Act)",
        ffs_rate_cents: 10520, // $105.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K032A",
        description: "Specific Neurocognitive Assessment (min 20 min)",
        ffs_rate_cents: 8225, // $82.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K033A",
        description: "Counselling \u{2014} Additional Units (per unit)",
        ffs_rate_cents: 5630, // $56.30
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K034A",
        description: "Telephone Reporting \u{2014} Specified Reportable Disease to MOH",
        ffs_rate_cents: 3600, // $36.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K035A",
        description: "Mandatory Reporting \u{2014} Medical Condition to Ontario MOT",
        ffs_rate_cents: 3625, // $36.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K036A",
        description: "Northern Health Travel Grant Application Form",
        ffs_rate_cents: 1025, // $10.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K037A",
        description: "Fibromyalgia/Myalgic Encephalomyelitis Care (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K038A",
        description: "Completion of LTC Health Assessment Form",
        ffs_rate_cents: 4635, // $46.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K039A",
        description: "Smoking Cessation Follow-Up Visit",
        ffs_rate_cents: 3925, // $39.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: Some(2),
    },

    // ── Shared Appointments (out-of-basket) ─────────────────────────────
    OhipCode {
        code: "K140A",
        description: "Shared Medical Appointment \u{2014} 2 Patients",
        ffs_rate_cents: 4005, // $40.05
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K141A",
        description: "Shared Medical Appointment \u{2014} 3 Patients",
        ffs_rate_cents: 2665, // $26.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K142A",
        description: "Shared Medical Appointment \u{2014} 4 Patients",
        ffs_rate_cents: 2015, // $20.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K143A",
        description: "Shared Medical Appointment \u{2014} 5 Patients",
        ffs_rate_cents: 1660, // $16.60
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K144A",
        description: "Shared Medical Appointment \u{2014} 6-12 Patients",
        ffs_rate_cents: 1410, // $14.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── eConsult (out-of-basket) ────────────────────────────────────────
    OhipCode {
        code: "K738A",
        description: "Physician-to-Physician eConsult \u{2014} Referring",
        ffs_rate_cents: 1645, // $16.45
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Prenatal/Obstetric (out-of-basket) ──────────────────────────────
    OhipCode {
        code: "P001A",
        description: "Attendance at Labour and Delivery (normal)",
        ffs_rate_cents: 47955, // $479.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P002A",
        description: "High Risk Prenatal Assessment",
        ffs_rate_cents: 8725, // $87.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P003A",
        description: "General Assessment (Major Prenatal Visit)",
        ffs_rate_cents: 9385, // $93.85
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P004A",
        description: "Minor Prenatal Assessment",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P005A",
        description: "Antenatal Preventive Health Assessment",
        ffs_rate_cents: 5570, // $55.70
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P006A",
        description: "Vaginal Delivery",
        ffs_rate_cents: 51265, // $512.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P007A",
        description: "Postnatal Care \u{2014} Hospital and/or Home",
        ffs_rate_cents: 6555, // $65.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P008A",
        description: "Postnatal Care \u{2014} Office",
        ffs_rate_cents: 4380, // $43.80
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P009A",
        description: "Attendance at Labour and Delivery (complicated)",
        ffs_rate_cents: 51265, // $512.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P018A",
        description: "Caesarean Section",
        ffs_rate_cents: 57980, // $579.80
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Incentives/Premiums (out-of-basket) ─────────────────────────────
    OhipCode {
        code: "Q040A",
        description: "Diabetes Management Incentive",
        ffs_rate_cents: 6570, // $65.70
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q042A",
        description: "Smoking Cessation Counselling Fee",
        ffs_rate_cents: 770, // $7.70
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q050A",
        description: "Heart Failure Management Incentive",
        ffs_rate_cents: 12500, // $125.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q053A",
        description: "HCC Complex/Vulnerable Patient Bonus",
        ffs_rate_cents: 35000, // $350.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q200A",
        description: "Per Patient Rostering Fee",
        ffs_rate_cents: 0, // $0.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── FHO+ Time-Based (out-of-basket) ─────────────────────────────────
    OhipCode {
        code: "Q310A",
        description: "Direct Patient Care \u{2014} In-Person/Video/Phone (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q311A",
        description: "Telephone Care \u{2014} Not in Office (per 15 min)",
        ffs_rate_cents: 1700, // $17.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q312A",
        description: "Indirect Patient Care \u{2014} Charting/Labs/Referrals (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q313A",
        description: "Clinical Administration \u{2014} EMR/QI/Screening (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q888A",
        description: "Weekend Office Access Premium (FHO)",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
];

// ── O(1) lookup map ────────────────────────────────────────────────────────

static CODE_MAP: LazyLock<HashMap<&'static str, &'static OhipCode>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(OHIP_CODES.len());
    for code in OHIP_CODES {
        m.insert(code.code, code);
    }
    m
});

/// Look up a single OHIP code by its code string (e.g. "A003A").
pub fn get_code(code: &str) -> Option<&'static OhipCode> {
    CODE_MAP.get(code).copied()
}

/// Return all codes that belong to a given category.
pub fn codes_by_category(cat: CodeCategory) -> Vec<&'static OhipCode> {
    OHIP_CODES
        .iter()
        .filter(|c| c.category == cat)
        .collect()
}

/// Return the full static code table.
pub fn all_codes() -> &'static [OhipCode] {
    OHIP_CODES
}

// ── Exclusion groups (mutual exclusivity rules) ─────────────────────────

/// Codes in the same exclusion group cannot be billed together in one encounter.
#[derive(Debug, Clone)]
pub struct ExclusionGroup {
    pub name: &'static str,
    pub codes: &'static [&'static str],
    pub reason: &'static str,
}

/// OHIP billing exclusion rules for FHO+ family medicine.
pub static EXCLUSION_GROUPS: &[ExclusionGroup] = &[
    ExclusionGroup {
        name: "Core assessments",
        codes: &["A001A", "A003A", "A004A", "A007A", "A008A"],
        reason: "Only one assessment code per visit",
    },
    ExclusionGroup {
        name: "Periodic health visits",
        codes: &["K130A", "K131A", "K132A", "K133A"],
        reason: "One periodic health visit per 12 months",
    },
    ExclusionGroup {
        name: "Assessment vs periodic",
        codes: &["A001A", "A003A", "A004A", "A007A", "A008A", "K130A", "K131A", "K132A", "K133A"],
        reason: "Assessment and periodic health visit are mutually exclusive",
    },
    ExclusionGroup {
        name: "K013 standalone",
        codes: &["K013A", "A001A", "A003A", "A004A", "A007A", "A008A"],
        reason: "K013 counselling cannot be billed with assessment codes",
    },
    ExclusionGroup {
        name: "Prenatal codes",
        codes: &["P003A", "P004A", "P005A"],
        reason: "One prenatal assessment type per visit",
    },
    ExclusionGroup {
        name: "Consultation types",
        codes: &["A005A", "A006A", "A905A", "C005A", "C006A", "C905A"],
        reason: "One consultation type per visit",
    },
    ExclusionGroup {
        name: "Malignant excision",
        codes: &["R048A", "R094A"],
        reason: "One excision code per lesion",
    },
    ExclusionGroup {
        name: "Laceration repair sizes",
        codes: &["Z154A", "Z175A", "Z176A"],
        reason: "One repair code per wound",
    },
    ExclusionGroup {
        name: "Group 1 excision vs electro",
        codes: &["Z156A", "Z157A", "Z158A", "Z159A", "Z160A", "Z161A"],
        reason: "Pick excision & suture OR electrocoagulation method \u{2014} not both",
    },
    ExclusionGroup {
        name: "Epistaxis treatment",
        codes: &["Z314A", "Z315A"],
        reason: "Cautery vs packing \u{2014} one per encounter",
    },
    // NOTE: G384A + G385A are a base+add-on pair — G385 REQUIRES G384.
    // They are NOT mutually exclusive. No exclusion group needed.
    ExclusionGroup {
        name: "Joint injection add-on",
        codes: &["G370A", "G371A"],
        reason: "G371 is add-on to G370 \u{2014} requires G370 as base code",
    },
    ExclusionGroup {
        name: "Direct care time",
        codes: &["Q310A", "Q311A"],
        reason: "In-office vs remote \u{2014} one setting per encounter",
    },
    ExclusionGroup {
        name: "FHO weekend access",
        codes: &["Q888A", "A888A"],
        reason: "Q888A and A888A cannot be billed same day",
    },
    ExclusionGroup {
        name: "SVP office premiums",
        codes: &["A990A", "A994A", "A996A", "A998A"],
        reason: "One SVP office premium per visit",
    },
    ExclusionGroup {
        name: "SVP home premiums",
        codes: &["B990A", "B992A", "B993A", "B994A", "B996A"],
        reason: "One SVP home premium per visit",
    },
    ExclusionGroup {
        name: "Prenatal visit types",
        codes: &["P001A", "P006A"],
        reason: "Individual visits vs global \u{2014} can't bill both",
    },
    ExclusionGroup {
        name: "Hospital assessment types",
        codes: &["C003A", "C004A"],
        reason: "Full vs partial admission assessment",
    },
];

/// Result of a conflict check.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResult {
    pub conflicting_code: String,
    pub group_name: String,
    pub reason: String,
}

/// Check if adding `new_code` conflicts with any of the `existing_codes`.
pub fn find_conflicts(existing_codes: &[&str], new_code: &str) -> Vec<ConflictResult> {
    let mut results = Vec::new();
    for group in EXCLUSION_GROUPS {
        if !group.codes.contains(&new_code) {
            continue;
        }
        for &existing in existing_codes {
            if existing == new_code {
                continue; // Don't conflict with self
            }
            if group.codes.contains(&existing) {
                // Check we haven't already added this conflict from another group
                if !results.iter().any(|r: &ConflictResult| r.conflicting_code == existing) {
                    results.push(ConflictResult {
                        conflicting_code: existing.to_string(),
                        group_name: group.name.to_string(),
                        reason: group.reason.to_string(),
                    });
                }
            }
        }
    }
    results
}

/// Check all codes in a list and return a map of code -> conflicting codes.
pub fn find_all_conflicts(codes: &[&str]) -> std::collections::HashMap<String, Vec<ConflictResult>> {
    let mut map = std::collections::HashMap::new();
    for (i, &code) in codes.iter().enumerate() {
        let others: Vec<&str> = codes.iter().enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, c)| *c)
            .collect();
        let conflicts = find_conflicts(&others, code);
        if !conflicts.is_empty() {
            map.insert(code.to_string(), conflicts);
        }
    }
    map
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_code_exists() {
        let a003 = get_code("A003A").expect("A003A should exist");
        assert_eq!(a003.description, "General Assessment");
        assert_eq!(a003.ffs_rate_cents, 9560);
        assert_eq!(a003.basket, Basket::In);
        assert_eq!(a003.shadow_pct, 30);
        assert!(a003.after_hours_eligible);
    }

    #[test]
    fn test_get_code_not_found() {
        assert!(get_code("ZZZZZ").is_none());
    }

    #[test]
    fn test_all_codes_non_empty() {
        assert!(!all_codes().is_empty());
    }

    #[test]
    fn test_code_count() {
        // 206 codes from April 2026 SOB (145 in-basket + 61 out-of-basket)
        assert_eq!(all_codes().len(), 206);
    }

    #[test]
    fn test_codes_by_category_time_based() {
        let time = codes_by_category(CodeCategory::TimeBased);
        assert_eq!(time.len(), 4);
        let codes: Vec<&str> = time.iter().map(|c| c.code).collect();
        assert!(codes.contains(&"Q310A"));
        assert!(codes.contains(&"Q311A"));
        assert!(codes.contains(&"Q312A"));
        assert!(codes.contains(&"Q313A"));
    }

    #[test]
    fn test_codes_by_category_immunization() {
        let imm = codes_by_category(CodeCategory::Immunization);
        // 11 in-basket (G462A + G538A + G840A-G848A) + 1 out-of-basket (G590A)
        assert_eq!(imm.len(), 12);
    }

    #[test]
    fn test_time_based_codes_are_out_basket() {
        for c in codes_by_category(CodeCategory::TimeBased) {
            assert_eq!(c.basket, Basket::Out);
            assert_eq!(c.shadow_pct, 100);
        }
    }

    #[test]
    fn test_all_in_basket_shadow_pct() {
        for c in all_codes() {
            if c.basket == Basket::In {
                assert!(
                    c.shadow_pct == 30 || c.shadow_pct == 50,
                    "In-basket code {} has shadow_pct={}, expected 30 or 50",
                    c.code,
                    c.shadow_pct
                );
            }
        }
    }

    #[test]
    fn test_out_of_basket_shadow_pct() {
        for c in all_codes() {
            if c.basket == Basket::Out {
                assert_eq!(
                    c.shadow_pct, 100,
                    "Out-of-basket code {} has shadow_pct={}, expected 100",
                    c.code, c.shadow_pct
                );
            }
        }
    }

    #[test]
    fn test_max_per_year_codes() {
        let k029 = get_code("K029A").unwrap();
        assert_eq!(k029.max_per_year, Some(6));
        let k039 = get_code("K039A").unwrap();
        assert_eq!(k039.max_per_year, Some(2));
        let k030 = get_code("K030A").unwrap();
        assert_eq!(k030.max_per_year, Some(4));
    }

    #[test]
    fn test_after_hours_eligible_assessment_codes() {
        // Only core A001-A008 assessments should be after-hours eligible
        for code in ["A001A", "A003A", "A004A", "A007A", "A008A"] {
            let c = get_code(code).unwrap();
            assert!(c.after_hours_eligible, "{} should be after-hours eligible", code);
        }
    }

    #[test]
    fn test_procedure_codes_not_after_hours() {
        for c in codes_by_category(CodeCategory::Procedure) {
            assert!(
                !c.after_hours_eligible,
                "Procedure {} should not be after-hours eligible",
                c.code
            );
        }
    }

    #[test]
    fn test_premium_codes() {
        let q053 = get_code("Q053A").unwrap();
        assert_eq!(q053.ffs_rate_cents, 35000); // $350.00
        assert_eq!(q053.basket, Basket::Out);
    }

    #[test]
    fn test_unique_codes() {
        let mut seen = std::collections::HashSet::new();
        for c in all_codes() {
            assert!(
                seen.insert(c.code),
                "Duplicate code found: {}",
                c.code
            );
        }
    }

    #[test]
    fn test_q310a_rate() {
        let q310 = get_code("Q310A").unwrap();
        assert_eq!(q310.ffs_rate_cents, 2000); // $20/15min
    }

    #[test]
    fn test_q311a_rate() {
        let q311 = get_code("Q311A").unwrap();
        assert_eq!(q311.ffs_rate_cents, 1700); // $17/15min
    }

    #[test]
    fn test_find_conflicts_assessment_mutual_exclusion() {
        let existing = vec!["A003A"];
        let conflicts = find_conflicts(&existing, "A004A");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].conflicting_code, "A003A");
    }

    #[test]
    fn test_find_conflicts_no_conflict() {
        let existing = vec!["A003A"];
        let conflicts = find_conflicts(&existing, "G365A"); // Pap smear doesn't conflict with assessment
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_find_conflicts_procedure_size() {
        let existing = vec!["R048A"]; // face/neck excision
        let conflicts = find_conflicts(&existing, "R094A"); // other areas excision
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].reason.contains("excision") || conflicts[0].reason.contains("lesion"));
    }

    #[test]
    fn test_find_conflicts_self_no_conflict() {
        let existing = vec!["A003A"];
        let conflicts = find_conflicts(&existing, "A003A");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_find_all_conflicts() {
        let codes = vec!["A003A", "A004A", "G365A"];
        let map = find_all_conflicts(&codes);
        assert!(map.contains_key("A003A"));
        assert!(map.contains_key("A004A"));
        assert!(!map.contains_key("G365A")); // Pap smear has no conflicts here
    }

    #[test]
    fn test_svp_premiums_in_basket() {
        for code in ["A990A", "A994A", "A996A", "A998A",
                     "B990A", "B992A", "B993A", "B994A", "B996A"] {
            let c = get_code(code).unwrap();
            assert_eq!(c.basket, Basket::In, "{} should be in-basket", code);
            assert_eq!(c.category, CodeCategory::Premium, "{} should be Premium", code);
            assert_eq!(c.shadow_pct, 30, "{} should have 30% shadow", code);
        }
    }

    #[test]
    fn test_sob_rates_key_codes() {
        // Verify key rates match April 2026 SOB
        assert_eq!(get_code("Z113A").unwrap().ffs_rate_cents, 3245); // $32.45
        assert_eq!(get_code("K023A").unwrap().ffs_rate_cents, 8525); // $85.25
        assert_eq!(get_code("P003A").unwrap().ffs_rate_cents, 9385); // $93.85
        assert_eq!(get_code("P004A").unwrap().ffs_rate_cents, 4455); // $44.55
        assert_eq!(get_code("Q040A").unwrap().ffs_rate_cents, 6570); // $65.70
    }
}
