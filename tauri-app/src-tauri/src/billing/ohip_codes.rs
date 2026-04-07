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
// Section 1: IN-BASKET codes (from OMA "Fee Codes in FHO and FHN Basket")
// Section 2: OUT-OF-BASKET codes (100% FFS)

pub static OHIP_CODES: &[OhipCode] = &[
    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 1: IN-BASKET CODES (OMA verified)
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
        description: "Intermediate Assessment / Well Baby Care",
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
        description: "Limited Virtual Care Service — Video",
        ffs_rate_cents: 2680, // $26.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A102A",
        description: "Limited Virtual Care Service — Phone",
        ffs_rate_cents: 2680, // $26.80
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
        description: "Intermediate Assessment — Pronouncement of Death",
        ffs_rate_cents: 4455, // $44.55 (FHO only)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A900A",
        description: "Complex House Call Assessment",
        ffs_rate_cents: 6480, // $64.80 (FHO only)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Focused Practice Assessments (in-basket, 30% shadow, Assessment) ─
    // Require FPA designation
    OhipCode {
        code: "A917A",
        description: "FPA — Sport Medicine",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A927A",
        description: "FPA — Allergy",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A937A",
        description: "FPA — Pain Management",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A947A",
        description: "FPA — Sleep Medicine",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A957A",
        description: "FPA — Addiction Medicine",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A967A",
        description: "FPA — Care of the Elderly",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Physician Office (in-basket, 30% shadow, Premium) ──────────
    OhipCode {
        code: "A990A",
        description: "SVP Office — Weekday Daytime (07:00-17:00)",
        ffs_rate_cents: 2055, // $20.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A994A",
        description: "SVP Office — Evening (17:00-24:00) Mon-Fri",
        ffs_rate_cents: 6170, // $61.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A996A",
        description: "SVP Office — Night (00:00-07:00)",
        ffs_rate_cents: 10280, // $102.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A998A",
        description: "SVP Office — Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 7710, // $77.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Patient's Home (in-basket, 30% shadow, Premium, FHO only) ──
    OhipCode {
        code: "B990A",
        description: "SVP Home — Weekday Daytime Non-elective/Elective",
        ffs_rate_cents: 2055, // $20.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B992A",
        description: "SVP Home — Weekday with Sacrifice of Office Hours",
        ffs_rate_cents: 3740, // $37.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B993A",
        description: "SVP Home — Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 7710, // $77.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B994A",
        description: "SVP Home — Evening (17:00-24:00) Mon-Fri Non-elective",
        ffs_rate_cents: 6170, // $61.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B996A",
        description: "SVP Home — Night (00:00-07:00) Non-elective",
        ffs_rate_cents: 10280, // $102.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Hospital (in-basket, 30% shadow, Assessment, FHO only) ───────────
    OhipCode {
        code: "C882A",
        description: "Palliative Care — Subsequent Visits by MRP from ICU Transfer",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C903A",
        description: "Pre-Dental/Pre-Operative General Assessment",
        ffs_rate_cents: 9560, // $95.60
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Tray Fee (in-basket, 50% shadow, Screening, FHO only) ───────────
    OhipCode {
        code: "E542A",
        description: "Tray Fee — When Procedure Performed Outside Hospital",
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
        description: "Lab — Cholesterol, Total",
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G002A",
        description: "Lab — Glucose, Quantitative/Semi-Quantitative",
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G004A",
        description: "Lab — Occult Blood",
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G005A",
        description: "Lab — Pregnancy Test",
        ffs_rate_cents: 450, // $4.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G009A",
        description: "Lab — Urinalysis, Routine (includes microscopy)",
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G010A",
        description: "Lab — Urinalysis Without Microscopy",
        ffs_rate_cents: 200, // $2.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G011A",
        description: "Lab — Fungus Culture incl KOH Prep and Smear",
        ffs_rate_cents: 450, // $4.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G012A",
        description: "Lab — Wet Preparation (fungus, trichomonas, parasites)",
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G014A",
        description: "Lab — Rapid Streptococcal Test",
        ffs_rate_cents: 450, // $4.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Allergy (in-basket, 50% shadow, Procedure) ──────────────────────
    OhipCode {
        code: "G197A",
        description: "Allergy — Skin Testing, Professional Component (max 50/yr)",
        ffs_rate_cents: 37, // $0.37
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G202A",
        description: "Allergy — Hyposensitisation, Each Injection",
        ffs_rate_cents: 445, // $4.45
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G205A",
        description: "Allergy — Insect Venom Desensitisation (max 5/day)",
        ffs_rate_cents: 1315, // $13.15
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G209A",
        description: "Allergy — Skin Testing, Technical Component (max 50/yr)",
        ffs_rate_cents: 72, // $0.72 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G212A",
        description: "Allergy — Hyposensitisation, Sole Reason for Visit",
        ffs_rate_cents: 975, // $9.75
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Nerve Blocks (in-basket, 50% shadow, Procedure, FHO only) ───────
    OhipCode {
        code: "G123A",
        description: "Nerve Block — Obturator, Each Additional (max 4)",
        ffs_rate_cents: 1555, // $15.55
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G223A",
        description: "Nerve Block — Somatic/Peripheral, Additional Nerve(s)",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G227A",
        description: "Nerve Block — Other Cranial Nerve",
        ffs_rate_cents: 5465, // $54.65
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G228A",
        description: "Nerve Block — Paravertebral (cervical/thoracic/lumbar/sacral/coccygeal)",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G231A",
        description: "Nerve Block — Somatic/Peripheral, One Nerve or Site",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G235A",
        description: "Nerve Block — Supraorbital",
        ffs_rate_cents: 3410, // $34.10
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Cardiovascular (in-basket, 30% shadow, Procedure) ───────────────
    OhipCode {
        code: "G271A",
        description: "Anticoagulant Supervision — Long-Term, Telephone/Month",
        ffs_rate_cents: 1395, // $13.95
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── ECG (in-basket, 50% shadow, Procedure, FHO only) ────────────────
    OhipCode {
        code: "G310A",
        description: "ECG — Twelve Lead, Technical Component",
        ffs_rate_cents: 1530, // $15.30
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G313A",
        description: "ECG — Twelve Lead, Professional Component (written interpretation)",
        ffs_rate_cents: 1530, // $15.30
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Gynaecology (in-basket, 50% shadow, Procedure) ──────────────────
    OhipCode {
        code: "G365A",
        description: "Papanicolaou Smear — Periodic",
        ffs_rate_cents: 1200, // $12.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G378A",
        description: "IUD Insertion",
        ffs_rate_cents: 4750, // $47.50 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G394A",
        description: "Papanicolaou Smear — Additional/Repeat",
        ffs_rate_cents: 1200, // $12.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G552A",
        description: "IUD Removal",
        ffs_rate_cents: 2380, // $23.80 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Injections/Infusions (in-basket, 50% shadow, Procedure) ─────────
    OhipCode {
        code: "G370A",
        description: "Injection/Aspiration of Joint, Bursa, Ganglion, or Tendon Sheath",
        ffs_rate_cents: 2025, // $20.25 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G371A",
        description: "Additional Joint/Bursa/Ganglion/Tendon Sheath (max 5)",
        ffs_rate_cents: 1990, // $19.90 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G372A",
        description: "IM/SC/Intradermal — Each Additional Injection (with visit)",
        ffs_rate_cents: 389, // $3.89
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G373A",
        description: "IM/SC/Intradermal — Sole Reason for Visit (first injection)",
        ffs_rate_cents: 675, // $6.75
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G375A",
        description: "Intralesional Infiltration — 1 or 2 Lesions",
        ffs_rate_cents: 885, // $8.85
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G377A",
        description: "Intralesional Infiltration — 3 or More Lesions",
        ffs_rate_cents: 1330, // $13.30
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G379A",
        description: "Intravenous — Child, Adolescent or Adult",
        ffs_rate_cents: 615, // $6.15
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G381A",
        description: "Chemotherapy — Standard Agents, Minor Toxicity",
        ffs_rate_cents: 5425, // $54.25 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G384A",
        description: "Trigger Point Injection — Infiltration of Tissue",
        ffs_rate_cents: 885, // $8.85
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G385A",
        description: "Trigger Point — Each Additional Site (max 2)",
        ffs_rate_cents: 455, // $4.55
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Other D&T (in-basket, various shadow) ────────────────────────────
    OhipCode {
        code: "G420A",
        description: "Ear Syringing/Curetting — Unilateral or Bilateral",
        ffs_rate_cents: 1595, // $15.95
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G435A",
        description: "Tonometry",
        ffs_rate_cents: 1025, // $10.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G462A",
        description: "Administration of Oral Polio Vaccine",
        ffs_rate_cents: 165, // $1.65
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
        ffs_rate_cents: 308, // $3.08
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G482A",
        description: "Venipuncture — Child",
        ffs_rate_cents: 450, // $4.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G489A",
        description: "Venipuncture — Adolescent or Adult",
        ffs_rate_cents: 450, // $4.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Audiometry (in-basket, 30% shadow, Screening) ───────────────────
    OhipCode {
        code: "G525A",
        description: "Pure Tone Threshold Audiometry — Professional Component",
        ffs_rate_cents: 1025, // $10.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Immunizations (in-basket, 50% shadow, Immunization) ─────────────
    OhipCode {
        code: "G538A",
        description: "Immunization — Other Agents Not Listed",
        ffs_rate_cents: 580, // $5.80
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G840A",
        description: "Immunization — DTaP/IPV (paediatric)",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G841A",
        description: "Immunization — DTaP-IPV-Hib (paediatric)",
        ffs_rate_cents: 635, // $6.35
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G842A",
        description: "Immunization — Hepatitis B",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G843A",
        description: "Immunization — Human Papillomavirus",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G844A",
        description: "Immunization — Meningococcal C Conjugate",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G845A",
        description: "Immunization — Measles, Mumps, Rubella",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G846A",
        description: "Immunization — Pneumococcal Conjugate",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G847A",
        description: "Immunization — Tdap (adult)",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G848A",
        description: "Immunization — Varicella",
        ffs_rate_cents: 540, // $5.40
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Spirometry (in-basket, 50% shadow, Procedure, FHO only) ─────────
    OhipCode {
        code: "J301A",
        description: "Spirometry — Simple (VC, FEV1, FEV1/FVC, MMEFR)",
        ffs_rate_cents: 2500, // $25.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J304A",
        description: "Flow Volume Loop — Expiratory + Inspiratory",
        ffs_rate_cents: 3500, // $35.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J324A",
        description: "Spirometry — Repeat After Bronchodilator",
        ffs_rate_cents: 1500, // $15.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "J327A",
        description: "Flow Volume Loop — Repeat After Bronchodilator",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Counselling/Mental Health (in-basket, 30% shadow, Counselling) ───
    OhipCode {
        code: "K001A",
        description: "Detention — Per Full Quarter Hour",
        ffs_rate_cents: 2680, // $26.80 (FHO only)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K002A",
        description: "Interviews with Relatives/Authorized Decision-Maker (per unit)",
        ffs_rate_cents: 8000, // $80.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K003A",
        description: "Interviews with CAS/Legal Guardian (per unit)",
        ffs_rate_cents: 8000, // $80.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K004A",
        description: "Psychotherapy — Family (2+ members, per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K005A",
        description: "Primary Mental Health Care — Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K006A",
        description: "Hypnotherapy — Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K007A",
        description: "Psychotherapy — Individual (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K008A",
        description: "Diagnostic Interview/Counselling — Child/Parent (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K013A",
        description: "Counselling — Individual (first 3 units K013+K040/12mo, per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K015A",
        description: "Counselling of Relatives — Terminally Ill Patient (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K017A",
        description: "Periodic Health Visit — Child",
        ffs_rate_cents: 4955, // $49.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Periodic Health Visits (in-basket, 30% shadow, Counselling) ──────
    OhipCode {
        code: "K130A",
        description: "Periodic Health Visit — Adolescent",
        ffs_rate_cents: 8710, // $87.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K131A",
        description: "Periodic Health Visit — Adult 18-64",
        ffs_rate_cents: 6425, // $64.25
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K132A",
        description: "Periodic Health Visit — Adult 65+",
        ffs_rate_cents: 9135, // $91.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K133A",
        description: "Periodic Health Visit — Adult with IDD",
        ffs_rate_cents: 9135, // $91.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Home Care (in-basket, 30% shadow, Counselling, FHO only) ────────
    OhipCode {
        code: "K070A",
        description: "Home Care Application/Supervision",
        ffs_rate_cents: 3475, // $34.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K071A",
        description: "Acute Home Care Supervision",
        ffs_rate_cents: 2140, // $21.40
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
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K702A",
        description: "Bariatric Out-Patient Case Conference (per unit)",
        ffs_rate_cents: 8000, // $80.00
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K730A",
        description: "Physician-to-Physician Phone Consultation — Referring",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K731A",
        description: "Physician-to-Physician Phone Consultation — Consultant",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K732A",
        description: "CritiCall Phone Consultation — Referring",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K733A",
        description: "CritiCall Phone Consultation — Consultant",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── SVP — Other Setting (in-basket, 30% shadow, Premium, FHO only) ──
    OhipCode {
        code: "Q990A",
        description: "SVP Other — Weekday Daytime (07:00-17:00)",
        ffs_rate_cents: 2055, // $20.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q992A",
        description: "SVP Other — Weekday with Sacrifice of Office Hours",
        ffs_rate_cents: 3740, // $37.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q994A",
        description: "SVP Other — Evening (17:00-24:00) Mon-Fri",
        ffs_rate_cents: 6170, // $61.70
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q996A",
        description: "SVP Other — Night (00:00-07:00)",
        ffs_rate_cents: 10280, // $102.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q998A",
        description: "SVP Other — Sat/Sun/Holiday (07:00-24:00)",
        ffs_rate_cents: 7710, // $77.10
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Integumentary Surgery (in-basket, 50% shadow, Procedure) ────────
    OhipCode {
        code: "R048A",
        description: "Malignant Lesion Excision — Face/Neck, Single",
        ffs_rate_cents: 6330, // $63.30 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R051A",
        description: "Malignant Lesion — Laser Surgery Group 1-4",
        ffs_rate_cents: 11085, // $110.85 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R094A",
        description: "Malignant Lesion Excision — Other Areas, Single",
        ffs_rate_cents: 14100, // $141.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z101A",
        description: "Abscess/Haematoma Incision — Subcutaneous, One",
        ffs_rate_cents: 3500, // $35.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z110A",
        description: "Onychogryphotic Nail — Extensive Debridement",
        ffs_rate_cents: 2000, // $20.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z113A",
        description: "Biopsy — Any Method, Without Sutures",
        ffs_rate_cents: 3500, // $35.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z114A",
        description: "Foreign Body Removal — Local Anaesthetic",
        ffs_rate_cents: 3500, // $35.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z116A",
        description: "Biopsy — Any Method, With Sutures",
        ffs_rate_cents: 5000, // $50.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z117A",
        description: "Chemical/Cryotherapy Treatment — One or More Lesions",
        ffs_rate_cents: 2250, // $22.50 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z122A",
        description: "Group 3 Excision (cyst/lipoma) — Face/Neck, Single",
        ffs_rate_cents: 6330, // $63.30 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z125A",
        description: "Group 3 Excision (cyst/lipoma) — Other Areas, Single",
        ffs_rate_cents: 6330, // $63.30 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z128A",
        description: "Nail Plate Excision Requiring Anaesthesia — One",
        ffs_rate_cents: 4500, // $45.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z129A",
        description: "Nail Plate Excision Requiring Anaesthesia — Multiple",
        ffs_rate_cents: 6000, // $60.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z154A",
        description: "Laceration Repair — Up to 5cm (face/layers)",
        ffs_rate_cents: 6330, // $63.30 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z156A",
        description: "Group 1 Excision (keratosis) — Excision & Suture, Single",
        ffs_rate_cents: 4000, // $40.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z157A",
        description: "Group 1 Excision (keratosis) — Excision & Suture, Two",
        ffs_rate_cents: 5500, // $55.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z158A",
        description: "Group 1 Excision (keratosis) — Excision & Suture, Three+",
        ffs_rate_cents: 7000, // $70.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z159A",
        description: "Group 1 — Electrocoagulation/Curetting, Single",
        ffs_rate_cents: 2250, // $22.50 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z160A",
        description: "Group 1 — Electrocoagulation/Curetting, Two",
        ffs_rate_cents: 3500, // $35.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z161A",
        description: "Group 1 — Electrocoagulation/Curetting, Three+",
        ffs_rate_cents: 4500, // $45.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z162A",
        description: "Group 2 (nevus) — Excision & Suture, Single",
        ffs_rate_cents: 5000, // $50.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z175A",
        description: "Laceration Repair — 5.1 to 10cm",
        ffs_rate_cents: 8660, // $86.60 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z176A",
        description: "Laceration Repair — Up to 5cm (simple)",
        ffs_rate_cents: 6330, // $63.30
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z314A",
        description: "Epistaxis — Cauterization, Unilateral",
        ffs_rate_cents: 2250, // $22.50 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z315A",
        description: "Epistaxis — Anterior Packing, Unilateral",
        ffs_rate_cents: 3500, // $35.00 (FHO only)
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── GI/Urological/Eye (in-basket, 50% shadow, Procedure, FHO only) ──
    OhipCode {
        code: "Z535A",
        description: "Sigmoidoscopy — Rigid Scope",
        ffs_rate_cents: 6115, // $61.15
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z543A",
        description: "Anoscopy (Proctoscopy)",
        ffs_rate_cents: 2250, // $22.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z545A",
        description: "Thrombosed Haemorrhoid(s) Incision",
        ffs_rate_cents: 6115, // $61.15
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z611A",
        description: "Catheterization — Hospital",
        ffs_rate_cents: 1500, // $15.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z847A",
        description: "Corneal Foreign Body Removal — Local Anaesthetic",
        ffs_rate_cents: 3500, // $35.00
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 2: OUT-OF-BASKET CODES (100% FFS)
    // ═══════════════════════════════════════════════════════════════════════

    // ── Consultations (out-of-basket, 100% FFS) ─────────────────────────
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
        description: "ED Equivalent — Weekend/Holiday",
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
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── House Calls (out-of-basket) ─────────────────────────────────────
    OhipCode {
        code: "A901A",
        description: "House Call Assessment",
        ffs_rate_cents: 6620, // $66.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A902A",
        description: "House Call — Pronouncement of Death",
        ffs_rate_cents: 6620, // $66.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A903A",
        description: "House Call — Additional Patient Same Residence",
        ffs_rate_cents: 3310, // $33.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Hospital Visits (out-of-basket) ─────────────────────────────────
    OhipCode {
        code: "C001A",
        description: "Family Practice Consultation",
        ffs_rate_cents: 6565, // $65.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C002A",
        description: "Repeat Consultation",
        ffs_rate_cents: 3835, // $38.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C003A",
        description: "Hospital Admission Assessment",
        ffs_rate_cents: 10930, // $109.30
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C004A",
        description: "Hospital Admission — Partial",
        ffs_rate_cents: 5845, // $58.45
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C009A",
        description: "Hospital Subsequent Visit",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C010A",
        description: "Hospital Concurrent Care",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C012A",
        description: "Hospital Discharge Day Management",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "H003A",
        description: "Newborn Hospital Care — First Day",
        ffs_rate_cents: 4475, // $44.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "H004A",
        description: "Newborn Hospital Care — Subsequent Day",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── LTC (out-of-basket) ─────────────────────────────────────────────
    OhipCode {
        code: "A191A",
        description: "LTC — New Admission Comprehensive",
        ffs_rate_cents: 16500, // $165.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A192A",
        description: "LTC — Subsequent Visit",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A193A",
        description: "LTC — Annual Comprehensive",
        ffs_rate_cents: 10930, // $109.30
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A194A",
        description: "LTC — Intermediate Visit",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A195A",
        description: "LTC — Pronouncement of Death",
        ffs_rate_cents: 6620, // $66.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Prenatal/Obstetric (out-of-basket) ──────────────────────────────
    OhipCode {
        code: "P001A",
        description: "Prenatal Visit — First",
        ffs_rate_cents: 6620, // $66.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P002A",
        description: "Prenatal Visit — Subsequent",
        ffs_rate_cents: 3310, // $33.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P003A",
        description: "Prenatal General Assessment",
        ffs_rate_cents: 8035, // $80.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P004A",
        description: "Prenatal Re-Assessment",
        ffs_rate_cents: 3815, // $38.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P005A",
        description: "Antenatal Preventive Assessment",
        ffs_rate_cents: 4515, // $45.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P006A",
        description: "Vaginal Delivery",
        ffs_rate_cents: 42820, // $428.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P007A",
        description: "Postnatal Visit",
        ffs_rate_cents: 3335, // $33.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P008A",
        description: "Postnatal — Subsequent",
        ffs_rate_cents: 2175, // $21.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P009A",
        description: "Prenatal Late Transfer In",
        ffs_rate_cents: 21200, // $212.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P013A",
        description: "Labour Management — First 2 Hours",
        ffs_rate_cents: 22000, // $220.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P014A",
        description: "Labour Management — Each Additional Hour",
        ffs_rate_cents: 5500, // $55.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P018A",
        description: "Postpartum Care — Comprehensive",
        ffs_rate_cents: 5845, // $58.45
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Chronic Disease (out-of-basket) ─────────────────────────────────
    OhipCode {
        code: "K022A",
        description: "HIV Primary Care (per unit, min 20 min)",
        ffs_rate_cents: 7010, // $70.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K023A",
        description: "Palliative Care Support",
        ffs_rate_cents: 6275, // $62.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K028A",
        description: "STI Management",
        ffs_rate_cents: 6275, // $62.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K029A",
        description: "Insulin Therapy Support (max 6/yr)",
        ffs_rate_cents: 3920, // $39.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(6),
    },
    OhipCode {
        code: "K030A",
        description: "Diabetic Management Assessment (max 4/yr)",
        ffs_rate_cents: 3920, // $39.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(4),
    },
    OhipCode {
        code: "K032A",
        description: "Neurocognitive Assessment (min 20 min)",
        ffs_rate_cents: 8225, // $82.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K033A",
        description: "Counselling — Additional Units (per unit)",
        ffs_rate_cents: 5630, // $56.30
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K039A",
        description: "Smoking Cessation Follow-Up (max 2/yr)",
        ffs_rate_cents: 3345, // $33.45
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(2),
    },

    // ── Palliative (out-of-basket) ──────────────────────────────────────
    OhipCode {
        code: "K036A",
        description: "Palliative Care Counselling — Office (half hour+)",
        ffs_rate_cents: 7525, // $75.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K037A",
        description: "Palliative Care Counselling — Subsequent",
        ffs_rate_cents: 3835, // $38.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K038A",
        description: "Palliative Care — Home Visit",
        ffs_rate_cents: 7525, // $75.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B998A",
        description: "Home Palliative Phone Management (per 15 min)",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E082A",
        description: "Palliative Care Premium (add-on)",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Geriatric (out-of-basket — may require FPA) ─────────────────────
    OhipCode {
        code: "K655A",
        description: "Comprehensive Geriatric Assessment (75+, annual)",
        ffs_rate_cents: 15655, // $156.55
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K656A",
        description: "Geriatric Assessment — Follow-Up",
        ffs_rate_cents: 7825, // $78.25
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Forms (out-of-basket) ───────────────────────────────────────────
    OhipCode {
        code: "K031A",
        description: "Certificate — Short (sick note)",
        ffs_rate_cents: 1595, // $15.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K034A",
        description: "Transfer of Care Summary",
        ffs_rate_cents: 4475, // $44.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K035A",
        description: "Certificate — Long (disability/insurance report)",
        ffs_rate_cents: 4475, // $44.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Shared Appointments (out-of-basket) ─────────────────────────────
    OhipCode {
        code: "K140A",
        description: "Shared Appointment — 2 Patients",
        ffs_rate_cents: 1670, // $16.70
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K141A",
        description: "Shared Appointment — 3 Patients",
        ffs_rate_cents: 1250, // $12.50
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K142A",
        description: "Shared Appointment — 4 Patients",
        ffs_rate_cents: 1000, // $10.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K143A",
        description: "Shared Appointment — 5 Patients",
        ffs_rate_cents: 835, // $8.35
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K144A",
        description: "Shared Appointment — 6+ Patients",
        ffs_rate_cents: 695, // $6.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Premiums/Incentives (out-of-basket) ─────────────────────────────
    OhipCode {
        code: "E079A",
        description: "Smoking Cessation — Initial Discussion (add-on)",
        ffs_rate_cents: 1595, // $15.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E430A",
        description: "Pap Tray Fee (with G365)",
        ffs_rate_cents: 1195, // $11.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E431A",
        description: "Pap Tray Fee (immunocompromised)",
        ffs_rate_cents: 1195, // $11.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q012A",
        description: "After-Hours Premium (50% of FFS)",
        ffs_rate_cents: 0, // percentage-based, not a fixed fee
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q040A",
        description: "Diabetes Management Incentive (after 3x K030)",
        ffs_rate_cents: 6000, // $60.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q042A",
        description: "Smoking Cessation Fee (add-on)",
        ffs_rate_cents: 750, // $7.50
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q050A",
        description: "CHF Management Incentive",
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
        code: "Q054A",
        description: "Mother & Newborn Bonus",
        ffs_rate_cents: 35000, // $350.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q010A",
        description: "Childhood Immunization Bonus",
        ffs_rate_cents: 620, // $6.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q015A",
        description: "Flu Immunization Bonus",
        ffs_rate_cents: 220, // $2.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q100A",
        description: "Cervical Screening Bonus",
        ffs_rate_cents: 640, // $6.40
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q101A",
        description: "Mammography Screening Bonus",
        ffs_rate_cents: 640, // $6.40
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q102A",
        description: "Colorectal Cancer Screening Bonus",
        ffs_rate_cents: 640, // $6.40
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q200A",
        description: "Patient Rostering Fee",
        ffs_rate_cents: 0, // $0
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
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

    // ── eConsult (out-of-basket) ────────────────────────────────────────
    OhipCode {
        code: "K738A",
        description: "eConsult — Specialist Seeking GP Input",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K998A",
        description: "Physician-to-Physician Phone Consultation",
        ffs_rate_cents: 2475, // $24.75
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Virtual Care Modality Indicators (out-of-basket, $0) ────────────
    OhipCode {
        code: "K300A",
        description: "Video Visit Modality Indicator",
        ffs_rate_cents: 0,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K301A",
        description: "Telephone Visit Modality Indicator",
        ffs_rate_cents: 0,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── FHO+ Time-Based (out-of-basket) ─────────────────────────────────
    OhipCode {
        code: "Q310",
        description: "Direct Patient Care (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q311",
        description: "Telephone Remote Care (per 15 min)",
        ffs_rate_cents: 1700, // $17.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q312",
        description: "Indirect Patient Care (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q313",
        description: "Clinical Administration (per 15 min)",
        ffs_rate_cents: 2000, // $20.00
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },

    // ── Influenza (FHN only, not FHO — treat as out-of-basket) ──────────
    OhipCode {
        code: "G590A",
        description: "Immunization — Influenza Agent (FHN only)",
        ffs_rate_cents: 565, // $5.65
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Immunization,
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
        codes: &["A005A", "A006A", "A905A"],
        reason: "One consultation type per visit",
    },
    ExclusionGroup {
        name: "Malignant excision",
        codes: &["R048A", "R051A", "R094A"],
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
        reason: "Pick excision & suture OR electrocoagulation method — not both",
    },
    ExclusionGroup {
        name: "Epistaxis treatment",
        codes: &["Z314A", "Z315A"],
        reason: "Cautery vs packing — one per encounter",
    },
    ExclusionGroup {
        name: "Trigger point add-on",
        codes: &["G384A", "G385A"],
        reason: "G385 is add-on to G384 — don't bill both as standalone",
    },
    ExclusionGroup {
        name: "Joint injection add-on",
        codes: &["G370A", "G371A"],
        reason: "G371 is add-on to G370 — requires G370 as base code",
    },
    ExclusionGroup {
        name: "Direct care time",
        codes: &["Q310", "Q311"],
        reason: "In-office vs remote — one setting per encounter",
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
        name: "SVP other premiums",
        codes: &["Q990A", "Q992A", "Q994A", "Q996A", "Q998A"],
        reason: "One SVP other premium per visit",
    },
    ExclusionGroup {
        name: "Prenatal visit types",
        codes: &["P001A", "P006A"],
        reason: "Individual visits vs global — can't bill both",
    },
    ExclusionGroup {
        name: "Hospital assessment types",
        codes: &["C003A", "C004A"],
        reason: "Full vs partial admission assessment",
    },
    ExclusionGroup {
        name: "Palliative counselling",
        codes: &["K036A", "K037A"],
        reason: "Initial vs subsequent per visit",
    },
    ExclusionGroup {
        name: "LTC assessment types",
        codes: &["A191A", "A193A"],
        reason: "Admission vs annual — different purposes but both comprehensive",
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

/// Check all codes in a list and return a map of code → conflicting codes.
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
        // Count all codes in the database
        assert_eq!(all_codes().len(), 226);
    }

    #[test]
    fn test_codes_by_category_time_based() {
        let time = codes_by_category(CodeCategory::TimeBased);
        assert_eq!(time.len(), 4);
        let codes: Vec<&str> = time.iter().map(|c| c.code).collect();
        assert!(codes.contains(&"Q310"));
        assert!(codes.contains(&"Q311"));
        assert!(codes.contains(&"Q312"));
        assert!(codes.contains(&"Q313"));
    }

    #[test]
    fn test_codes_by_category_immunization() {
        let imm = codes_by_category(CodeCategory::Immunization);
        // 11 in-basket + 1 out-of-basket (G590A FHN-only)
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
        let q012 = get_code("Q012A").unwrap();
        assert_eq!(q012.ffs_rate_cents, 0); // percentage-based
        assert_eq!(q012.category, CodeCategory::Premium);

        let q053 = get_code("Q053A").unwrap();
        assert_eq!(q053.ffs_rate_cents, 35000); // $350.00
        assert_eq!(q053.basket, Basket::Out);

        let q054 = get_code("Q054A").unwrap();
        assert_eq!(q054.ffs_rate_cents, 35000); // $350.00
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
    fn test_q310_rate() {
        let q310 = get_code("Q310").unwrap();
        assert_eq!(q310.ffs_rate_cents, 2000); // $20/15min
        assert_eq!(q310.description, "Direct Patient Care (per 15 min)");
    }

    #[test]
    fn test_q311_rate() {
        let q311 = get_code("Q311").unwrap();
        assert_eq!(q311.ffs_rate_cents, 1700); // $17/15min
        assert_eq!(q311.description, "Telephone Remote Care (per 15 min)");
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
        let conflicts = find_conflicts(&existing, "R051A"); // laser surgery
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
    fn test_removed_codes_gone() {
        // Verify removed codes no longer exist
        for code in ["G369A", "G591A", "Z104A", "Z108A", "Z112A", "Z119A",
                     "Z169A", "Z200A", "Z201A", "E502A", "E540A", "E541A",
                     "K197A", "B960A", "B961A", "B962A"] {
            assert!(get_code(code).is_none(), "{} should have been removed", code);
        }
    }

    #[test]
    fn test_new_in_basket_codes_exist() {
        // Verify key new in-basket codes exist
        for code in ["A101A", "A102A", "A110A", "A112A", "A777A",
                     "A917A", "A990A", "A994A", "A996A", "A998A",
                     "B990A", "B992A", "B993A", "B994A", "B996A",
                     "C882A", "C903A", "E542A",
                     "G001A", "G002A", "G004A", "G005A", "G009A",
                     "G197A", "G202A", "G205A", "G209A", "G212A",
                     "G123A", "G223A", "G227A", "G235A", "G271A",
                     "G310A", "G377A", "G381A",
                     "G435A", "G481A", "G482A", "G489A", "G525A",
                     "G840A", "G841A", "G842A", "G843A", "G844A",
                     "G845A", "G846A", "G847A", "G848A",
                     "J301A", "J304A", "J324A", "J327A",
                     "K001A", "K003A", "K004A", "K006A", "K008A",
                     "K700A", "K702A", "K730A", "K731A", "K732A", "K733A",
                     "Q990A", "Q992A", "Q994A", "Q996A", "Q998A",
                     "Z110A", "Z116A", "Z122A", "Z125A", "Z128A",
                     "Z156A", "Z157A", "Z158A", "Z159A", "Z161A",
                     "Z162A", "Z175A", "Z611A"] {
            assert!(get_code(code).is_some(), "{} should exist", code);
        }
    }

    #[test]
    fn test_svp_premiums_in_basket() {
        for code in ["A990A", "A994A", "A996A", "A998A",
                     "B990A", "B992A", "B993A", "B994A", "B996A",
                     "Q990A", "Q992A", "Q994A", "Q996A", "Q998A"] {
            let c = get_code(code).unwrap();
            assert_eq!(c.basket, Basket::In, "{} should be in-basket", code);
            assert_eq!(c.category, CodeCategory::Premium, "{} should be Premium", code);
            assert_eq!(c.shadow_pct, 30, "{} should have 30% shadow", code);
        }
    }
}
