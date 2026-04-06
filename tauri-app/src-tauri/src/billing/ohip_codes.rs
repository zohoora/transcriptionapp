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

pub static OHIP_CODES: &[OhipCode] = &[
    // ── Assessments (in-basket, 30% shadow) ────────────────────────────────
    OhipCode {
        code: "A001A",
        description: "Minor Assessment",
        ffs_rate_cents: 2375,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A003A",
        description: "General Assessment",
        ffs_rate_cents: 7720,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A004A",
        description: "General Re-Assessment",
        ffs_rate_cents: 3815,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A007A",
        description: "Intermediate Assessment / Well Baby",
        ffs_rate_cents: 3370,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A008A",
        description: "Mini Assessment",
        ffs_rate_cents: 1590,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A888A",
        description: "Weekend/Holiday Special Visit",
        ffs_rate_cents: 3370,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    // ── Counselling (in-basket, 30% shadow) ────────────────────────────────
    OhipCode {
        code: "K005A",
        description: "Individual Counselling (per unit)",
        ffs_rate_cents: 2170,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K013A",
        description: "Counselling Extended",
        ffs_rate_cents: 6275,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K017A",
        description: "Antenatal Preventive Assessment",
        ffs_rate_cents: 4515,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K033A",
        description: "Additional Counselling",
        ffs_rate_cents: 3815,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K130A",
        description: "Periodic Health Visit (18-44)",
        ffs_rate_cents: 5000,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K131A",
        description: "Periodic Health Visit (45-64)",
        ffs_rate_cents: 5000,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K132A",
        description: "Periodic Health Visit (65+)",
        ffs_rate_cents: 5000,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Select Procedures (in-basket, 50% shadow) ─────────────────────────
    OhipCode {
        code: "G365A",
        description: "Pap Smear",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G378A",
        description: "IUD Insertion",
        ffs_rate_cents: 6115,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G552A",
        description: "IUD Removal",
        ffs_rate_cents: 2250,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R048A",
        description: "Malignant Lesion Excision (small)",
        ffs_rate_cents: 6330,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R051A",
        description: "Malignant Lesion Excision (medium)",
        ffs_rate_cents: 11085,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R094A",
        description: "Malignant Lesion Excision (large)",
        ffs_rate_cents: 14100,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z101A",
        description: "Abscess I&D",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z104A",
        description: "Skin Biopsy",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z108A",
        description: "Cryotherapy (single)",
        ffs_rate_cents: 2250,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z110A",
        description: "Cryotherapy (2-5 lesions)",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z112A",
        description: "Electrocoagulation (single)",
        ffs_rate_cents: 2250,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z113A",
        description: "Electrocoagulation (2-5)",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z114A",
        description: "Excision Benign Lesion (small)",
        ffs_rate_cents: 6330,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z119A",
        description: "Excision Benign Lesion (medium)",
        ffs_rate_cents: 11085,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z154A",
        description: "Suture Laceration (simple <5cm)",
        ffs_rate_cents: 6330,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z160A",
        description: "Suture Laceration (simple 5-10cm)",
        ffs_rate_cents: 8660,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z176A",
        description: "Suture Laceration (complex)",
        ffs_rate_cents: 14100,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z314A",
        description: "Epistaxis Cautery",
        ffs_rate_cents: 2250,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z315A",
        description: "Epistaxis Anterior Packing",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z535A",
        description: "Sigmoidoscopy",
        ffs_rate_cents: 6115,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z543A",
        description: "Anoscopy",
        ffs_rate_cents: 2250,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z545A",
        description: "Thrombosed Hemorrhoid Incision",
        ffs_rate_cents: 6115,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z847A",
        description: "Corneal Foreign Body Removal",
        ffs_rate_cents: 3500,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Immunizations (in-basket, 50% shadow) ─────────────────────────────
    OhipCode {
        code: "G462A",
        description: "Travel Immunization",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G538A",
        description: "Immunization General",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G840A",
        description: "Influenza Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G841A",
        description: "Pneumococcal Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G842A",
        description: "Hepatitis B Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G843A",
        description: "MMR Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G844A",
        description: "Td/Tdap Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G848A",
        description: "Other Vaccine Admin",
        ffs_rate_cents: 1200,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Screening (in-basket, 30% shadow) ─────────────────────────────────
    OhipCode {
        code: "G590A",
        description: "Colorectal Screening Discussion",
        ffs_rate_cents: 686,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G591A",
        description: "Breast Screening Discussion",
        ffs_rate_cents: 686,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E430A",
        description: "Tray Fee (with Pap G365)",
        ffs_rate_cents: 1195,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E431A",
        description: "Tray Fee (with Pap G394)",
        ffs_rate_cents: 1195,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E079A",
        description: "Smoking Cessation Discussion (add-on)",
        ffs_rate_cents: 750,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Out-of-Basket (100% FFS) ──────────────────────────────────────────
    OhipCode {
        code: "P003A",
        description: "Prenatal General Assessment",
        ffs_rate_cents: 8035,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P004A",
        description: "Prenatal Re-Assessment",
        ffs_rate_cents: 3815,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P005A",
        description: "Antenatal Preventive Health",
        ffs_rate_cents: 4515,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K028A",
        description: "STI Management",
        ffs_rate_cents: 6275,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K029A",
        description: "Insulin Therapy Support",
        ffs_rate_cents: 3920,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(6),
    },
    OhipCode {
        code: "K023A",
        description: "Palliative Care Support",
        ffs_rate_cents: 6275,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K039A",
        description: "Smoking Cessation Follow-Up",
        ffs_rate_cents: 3345,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(2),
    },
    OhipCode {
        code: "K070A",
        description: "Home Care Application",
        ffs_rate_cents: 5000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K071A",
        description: "Acute Home Care Supervision",
        ffs_rate_cents: 5000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K032A",
        description: "Neurocognitive Assessment",
        ffs_rate_cents: 6275,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K140A",
        description: "Shared Appointment (2 patients)",
        ffs_rate_cents: 1670,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K141A",
        description: "Shared Appointment (3 patients)",
        ffs_rate_cents: 1250,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K142A",
        description: "Shared Appointment (4 patients)",
        ffs_rate_cents: 1000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K143A",
        description: "Shared Appointment (5 patients)",
        ffs_rate_cents: 835,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K144A",
        description: "Shared Appointment (6+ patients)",
        ffs_rate_cents: 695,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q040A",
        description: "Diabetes Management Incentive",
        ffs_rate_cents: 6000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q042A",
        description: "Smoking Cessation Fee (add-on)",
        ffs_rate_cents: 750,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q050A",
        description: "CHF Management Incentive",
        ffs_rate_cents: 12500,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Premiums ───────────────────────────────────────────────────────────
    OhipCode {
        code: "Q012A",
        description: "After-Hours Premium",
        ffs_rate_cents: 0, // percentage-based, not a fixed fee
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q053A",
        description: "Patient Attachment Bonus",
        ffs_rate_cents: 50000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q054A",
        description: "Mother & Newborn Bonus",
        ffs_rate_cents: 35000,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Time-Based ─────────────────────────────────────────────────────────
    OhipCode {
        code: "Q310",
        description: "Direct Patient Care",
        ffs_rate_cents: 2000, // $20.00 per 15 min
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q311",
        description: "Telephone Remote",
        ffs_rate_cents: 1700, // $17.00 per 15 min
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q312",
        description: "Indirect Patient Care",
        ffs_rate_cents: 2000, // $20.00 per 15 min
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q313",
        description: "Clinical Administration",
        ffs_rate_cents: 2000, // $20.00 per 15 min
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::TimeBased,
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

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_code_exists() {
        let a003 = get_code("A003A").expect("A003A should exist");
        assert_eq!(a003.description, "General Assessment");
        assert_eq!(a003.ffs_rate_cents, 7720);
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
        // Verify we have the expected number of codes
        // 6 assessments + 7 counselling + 23 procedures + 8 immunizations
        // + 5 screening + 18 out-of-basket + 3 premiums + 4 time-based = 74
        assert_eq!(all_codes().len(), 74);
    }

    #[test]
    fn test_codes_by_category_assessment() {
        let assessments = codes_by_category(CodeCategory::Assessment);
        // 6 in-basket + 3 out-of-basket (P003A, P004A, P005A) = 9
        assert_eq!(assessments.len(), 9);
        for a in &assessments {
            assert_eq!(a.category, CodeCategory::Assessment);
        }
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
        assert_eq!(imm.len(), 8);
        for i in &imm {
            assert_eq!(i.basket, Basket::In);
            assert_eq!(i.shadow_pct, 50);
        }
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
    }

    #[test]
    fn test_after_hours_eligible_assessment_codes() {
        // All A-prefix assessments should be after-hours eligible
        for code in ["A001A", "A003A", "A004A", "A007A", "A008A", "A888A"] {
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
        assert_eq!(q053.ffs_rate_cents, 50000); // $500
        assert_eq!(q053.basket, Basket::Out);

        let q054 = get_code("Q054A").unwrap();
        assert_eq!(q054.ffs_rate_cents, 35000); // $350
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
        assert_eq!(q310.description, "Direct Patient Care");
    }

    #[test]
    fn test_q311_rate() {
        let q311 = get_code("Q311").unwrap();
        assert_eq!(q311.ffs_rate_cents, 1700); // $17/15min
        assert_eq!(q311.description, "Telephone Remote");
    }
}
