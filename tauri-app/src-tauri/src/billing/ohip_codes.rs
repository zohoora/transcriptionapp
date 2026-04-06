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
        ffs_rate_cents: 2680,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A003A",
        description: "General Assessment",
        ffs_rate_cents: 9560,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A004A",
        description: "General Re-Assessment",
        ffs_rate_cents: 3935,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A007A",
        description: "Intermediate Assessment / Well Baby",
        ffs_rate_cents: 4455,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A008A",
        description: "Mini Assessment",
        ffs_rate_cents: 1340,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A888A",
        description: "Weekend/Holiday Special Visit",
        ffs_rate_cents: 4455,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    // ── Counselling (in-basket, 30% shadow) ────────────────────────────────
    OhipCode {
        code: "K005A",
        description: "Primary Mental Health Care Counselling (per unit)",
        ffs_rate_cents: 8000,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K013A",
        description: "Counselling (first 3 units/year, per unit)",
        ffs_rate_cents: 8000,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K017A",
        description: "Antenatal Preventive Assessment",
        ffs_rate_cents: 4955,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K033A",
        description: "Additional Counselling",
        ffs_rate_cents: 5630,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K130A",
        description: "Periodic Health Visit (18-44)",
        ffs_rate_cents: 8710,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K131A",
        description: "Periodic Health Visit (45-64)",
        ffs_rate_cents: 6425,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K132A",
        description: "Periodic Health Visit (65+)",
        ffs_rate_cents: 9135,
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
        ffs_rate_cents: 4750,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G552A",
        description: "IUD Removal",
        ffs_rate_cents: 2380,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R048A",
        description: "Malignant Lesion Excision (small)",
        ffs_rate_cents: 6670,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R051A",
        description: "Malignant Lesion Excision (medium)",
        ffs_rate_cents: 11680,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "R094A",
        description: "Malignant Lesion Excision (large)",
        ffs_rate_cents: 14857,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z101A",
        description: "Abscess I&D",
        ffs_rate_cents: 3688,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z104A",
        description: "Skin Biopsy",
        ffs_rate_cents: 3688,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z108A",
        description: "Cryotherapy (single)",
        ffs_rate_cents: 2371,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z110A",
        description: "Cryotherapy (2-5 lesions)",
        ffs_rate_cents: 3688,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z112A",
        description: "Electrocoagulation (single)",
        ffs_rate_cents: 2371,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z113A",
        description: "Electrocoagulation (2-5)",
        ffs_rate_cents: 3688,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z114A",
        description: "Excision Benign Lesion (small)",
        ffs_rate_cents: 6670,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z119A",
        description: "Excision Benign Lesion (medium)",
        ffs_rate_cents: 11680,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z154A",
        description: "Suture Laceration (simple <5cm)",
        ffs_rate_cents: 6670,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z160A",
        description: "Suture Laceration (simple 5-10cm)",
        ffs_rate_cents: 9125,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z176A",
        description: "Suture Laceration (complex)",
        ffs_rate_cents: 14857,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z314A",
        description: "Epistaxis Cautery",
        ffs_rate_cents: 2371,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z315A",
        description: "Epistaxis Anterior Packing",
        ffs_rate_cents: 3688,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z535A",
        description: "Sigmoidoscopy",
        ffs_rate_cents: 6443,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z543A",
        description: "Anoscopy",
        ffs_rate_cents: 2371,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z545A",
        description: "Thrombosed Hemorrhoid Incision",
        ffs_rate_cents: 6443,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z847A",
        description: "Corneal Foreign Body Removal",
        ffs_rate_cents: 3688,
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
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G538A",
        description: "Immunization General",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G840A",
        description: "Influenza Vaccine Admin",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G841A",
        description: "Pneumococcal Vaccine Admin",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G842A",
        description: "Hepatitis B Vaccine Admin",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G843A",
        description: "MMR Vaccine Admin",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G844A",
        description: "Td/Tdap Vaccine Admin",
        ffs_rate_cents: 880,
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Immunization,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G848A",
        description: "Other Vaccine Admin",
        ffs_rate_cents: 880,
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
        ffs_rate_cents: 880,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G591A",
        description: "Breast Screening Discussion (verify)",
        ffs_rate_cents: 723,
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
        ffs_rate_cents: 1259,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E079A",
        description: "Smoking Cessation Discussion (add-on)",
        ffs_rate_cents: 1595,
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
        ffs_rate_cents: 8467,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P004A",
        description: "Prenatal Re-Assessment",
        ffs_rate_cents: 4020,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P005A",
        description: "Antenatal Preventive Health",
        ffs_rate_cents: 4758,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K028A",
        description: "STI Management",
        ffs_rate_cents: 6612,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K029A",
        description: "Insulin Therapy Support",
        ffs_rate_cents: 4131,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(6),
    },
    OhipCode {
        code: "K023A",
        description: "Palliative Care Support",
        ffs_rate_cents: 6612,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K039A",
        description: "Smoking Cessation Follow-Up",
        ffs_rate_cents: 3525,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: Some(2),
    },
    OhipCode {
        code: "K070A",
        description: "Home Care Application",
        ffs_rate_cents: 3475,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K071A",
        description: "Acute Home Care Supervision",
        ffs_rate_cents: 5269,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K032A",
        description: "Neurocognitive Assessment",
        ffs_rate_cents: 8225,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K140A",
        description: "Shared Appointment (2 patients)",
        ffs_rate_cents: 1760,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K141A",
        description: "Shared Appointment (3 patients)",
        ffs_rate_cents: 1317,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K142A",
        description: "Shared Appointment (4 patients)",
        ffs_rate_cents: 1054,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K143A",
        description: "Shared Appointment (5 patients)",
        ffs_rate_cents: 880,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K144A",
        description: "Shared Appointment (6+ patients)",
        ffs_rate_cents: 732,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q040A",
        description: "Diabetes Management Incentive",
        ffs_rate_cents: 6322,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q042A",
        description: "Smoking Cessation Fee (add-on)",
        ffs_rate_cents: 790,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q050A",
        description: "CHF Management Incentive",
        ffs_rate_cents: 13171,
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
        ffs_rate_cents: 52685,
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q054A",
        description: "Mother & Newborn Bonus",
        ffs_rate_cents: 36880,
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
    // ── Injections & Joint Procedures (in-basket, 50% shadow) ──────────
    OhipCode {
        code: "G373A",
        description: "Injection — Sole Reason for Visit",
        ffs_rate_cents: 711, // $7.11
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G369A",
        description: "Epidural Injection — Caudal or Lumbar",
        ffs_rate_cents: 6612, // $66.12
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G370A",
        description: "Nerve Block — Peripheral",
        ffs_rate_cents: 3688, // $36.88
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G371A",
        description: "Trigger Point Injection (single site)",
        ffs_rate_cents: 2371, // $23.71
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G372A",
        description: "Trigger Point Injection (multiple sites)",
        ffs_rate_cents: 3688, // $36.88
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z331A",
        description: "Intra-articular Joint Injection (small joint)",
        ffs_rate_cents: 2371, // $23.71
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z332A",
        description: "Intra-articular Joint Injection (large joint — knee, shoulder)",
        ffs_rate_cents: 3688, // $36.88
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G394A",
        description: "Pap Smear — Repeat/Follow-up",
        ffs_rate_cents: 1264, // $12.64
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Additional Common FP Procedures (in-basket, 50% shadow) ────────
    OhipCode {
        code: "Z117A",
        description: "Wound Care — Debridement",
        ffs_rate_cents: 3688, // $36.88
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z129A",
        description: "Removal of Foreign Body — Skin/Subcutaneous",
        ffs_rate_cents: 3688, // $36.88
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z169A",
        description: "Toenail Removal — Partial or Complete",
        ffs_rate_cents: 6459, // $64.59
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E502A",
        description: "Tray Fee — Minor Procedure",
        ffs_rate_cents: 1259, // $12.59
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Screening,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Additional Out-of-Basket Services ──────────────────────────────
    OhipCode {
        code: "K030A",
        description: "Diabetic Management Assessment (max 4/year)",
        ffs_rate_cents: 4131, // $41.31
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: Some(4),
    },
    OhipCode {
        code: "A900A",
        description: "Complex House Call Assessment — Frail/Housebound",
        ffs_rate_cents: 6480, // $64.80
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K022A",
        description: "HIV Primary Care (per unit, min 20 min)",
        ffs_rate_cents: 7010, // $70.10
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Special Visit / House Call Premiums (in-basket, 30% shadow) ────────
    OhipCode {
        code: "B960A",
        description: "Special Visit Premium — House Call Weekday Daytime",
        ffs_rate_cents: 3640, // $36.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "B961A",
        description: "Special Visit Premium — House Call with Sacrifice of Office Hours",
        ffs_rate_cents: 3640, // $36.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "B962A",
        description: "Special Visit Premium — House Call Evening",
        ffs_rate_cents: 3640, // $36.40
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: true,
        max_per_year: None,
    },
    // ── Mental Health (in-basket, 30% shadow) ────────────────────────────
    OhipCode {
        code: "K007A",
        description: "Psychotherapy — Individual (half hour+)",
        ffs_rate_cents: 7650, // $76.50
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K002A",
        description: "Individual Psychotherapy (half hour)",
        ffs_rate_cents: 5969, // $59.69
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "K197A",
        description: "Individual Psychotherapy (primarily psychiatric, verify for GP use)",
        ffs_rate_cents: 8030, // $80.30
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Hospital Visits (out-of-basket, 100% FFS) ────────────────────────
    OhipCode {
        code: "C003A",
        description: "Hospital Admission Assessment",
        ffs_rate_cents: 11517, // $115.17
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "C004A",
        description: "Hospital Admission — Partial Assessment",
        ffs_rate_cents: 6159, // $61.59
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "C009A",
        description: "Hospital Subsequent Visit",
        ffs_rate_cents: 3514, // $35.14
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "C010A",
        description: "Hospital Concurrent Care",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C012A",
        description: "Hospital Discharge Day Management",
        ffs_rate_cents: 3514, // $35.14
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "C001A",
        description: "Family Practice Consultation",
        ffs_rate_cents: 6918, // $69.18
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "C002A",
        description: "Repeat Consultation",
        ffs_rate_cents: 4041, // $40.41
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "H003A",
        description: "Newborn Hospital Care — First Day",
        ffs_rate_cents: 4715, // $47.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "H004A",
        description: "Newborn Hospital Care — Subsequent Day",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Long-Term Care (out-of-basket, 100% FFS) ─────────────────────────
    OhipCode {
        code: "A191A",
        description: "LTC New Admission Comprehensive Assessment",
        ffs_rate_cents: 17386, // $173.86
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A192A",
        description: "LTC Subsequent Visit",
        ffs_rate_cents: 3514, // $35.14
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A193A",
        description: "LTC Annual Comprehensive Assessment",
        ffs_rate_cents: 11517, // $115.17
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A194A",
        description: "LTC Intermediate Visit",
        ffs_rate_cents: 3514, // $35.14
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A195A",
        description: "LTC Pronouncement of Death",
        ffs_rate_cents: 6976, // $69.76
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── House Calls (out-of-basket, 100% FFS) ────────────────────────────
    OhipCode {
        code: "A901A",
        description: "House Call Assessment",
        ffs_rate_cents: 6976, // $69.76
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: true,
        max_per_year: None,
    },
    OhipCode {
        code: "A902A",
        description: "House Call — Pronouncement of Death",
        ffs_rate_cents: 6976, // $69.76
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "A903A",
        description: "House Call — Additional Patient Same Residence",
        ffs_rate_cents: 3488, // $34.88
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Consultations (out-of-basket, 100% FFS) ────────────────────────
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
        code: "A905A",
        description: "Limited Consultation",
        ffs_rate_cents: 8015, // $80.15
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Prenatal / Obstetric ─────────────────────────────────────────────
    OhipCode {
        code: "P001A",
        description: "Prenatal Visit — First",
        ffs_rate_cents: 6976, // $69.76
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P002A",
        description: "Prenatal Visit — Subsequent",
        ffs_rate_cents: 3488, // $34.88
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P006A",
        description: "Vaginal Delivery — Full Labour and Delivery",
        ffs_rate_cents: 42820, // $428.20
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P007A",
        description: "Postnatal Visit — General Assessment",
        ffs_rate_cents: 3514, // $35.14
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P008A",
        description: "Postnatal Visit — Subsequent",
        ffs_rate_cents: 2292, // $22.92
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P009A",
        description: "Prenatal Care — Late Transfer In",
        ffs_rate_cents: 22338, // $223.38
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P018A",
        description: "Postpartum Care — Comprehensive",
        ffs_rate_cents: 6159, // $61.59
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P013A",
        description: "Labour Management — First 2 Hours",
        ffs_rate_cents: 23181, // $231.81
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "P014A",
        description: "Labour Management — Each Additional Hour",
        ffs_rate_cents: 5795, // $57.95
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Palliative Care (out-of-basket, 100% FFS) ────────────────────────
    OhipCode {
        code: "K036A",
        description: "Palliative Care Counselling (office, half hour+)",
        ffs_rate_cents: 7929, // $79.29
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K037A",
        description: "Palliative Care Counselling — Subsequent",
        ffs_rate_cents: 4041, // $40.41
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K038A",
        description: "Palliative Care — Home Visit",
        ffs_rate_cents: 7929, // $79.29
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E082A",
        description: "Palliative Care Premium (add-on to visit)",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "B998A",
        description: "Home Palliative Phone Management (per 15 min)",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::ChronicDisease,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Geriatric (in-basket, 30% shadow) ────────────────────────────────
    OhipCode {
        code: "K655A",
        description: "Comprehensive Geriatric Assessment (75+, annual)",
        ffs_rate_cents: 15655, // $156.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K656A",
        description: "Geriatric Assessment — Follow-Up",
        ffs_rate_cents: 8245, // $82.45
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Assessment,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Preventive Care Bonuses (out-of-basket, 100% FFS) ────────────────
    OhipCode {
        code: "Q010A",
        description: "Childhood Immunization Bonus (per series)",
        ffs_rate_cents: 653, // $6.53
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q015A",
        description: "Flu Immunization Bonus",
        ffs_rate_cents: 232, // $2.32
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q100A",
        description: "Cervical Screening Bonus (Pap referral)",
        ffs_rate_cents: 674, // $6.74
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q101A",
        description: "Mammography Screening Bonus (referral)",
        ffs_rate_cents: 674, // $6.74
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q102A",
        description: "Colorectal Cancer Screening Bonus (FOBT/FIT)",
        ffs_rate_cents: 674, // $6.74
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q200A",
        description: "New Patient Intake Incentive",
        ffs_rate_cents: 6533, // $65.33
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Form Completion (in-basket, 30% shadow) ──────────────────────────
    OhipCode {
        code: "K031A",
        description: "Certificate — Short (sick note, return-to-work)",
        ffs_rate_cents: 1681, // $16.81
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K035A",
        description: "Certificate — Long (insurance, disability report)",
        ffs_rate_cents: 4715, // $47.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K034A",
        description: "Transfer of Care Summary/Report",
        ffs_rate_cents: 4715, // $47.15
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Additional Procedures (in-basket, 50% shadow) ────────────────────
    OhipCode {
        code: "G420A",
        description: "Ear Syringing (cerumen removal, bilateral)",
        ffs_rate_cents: 1681, // $16.81
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G313A",
        description: "Aspiration — Abscess/Cyst/Hematoma",
        ffs_rate_cents: 3941, // $39.41
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z200A",
        description: "Curette — Skin Lesion (shave/curettage)",
        ffs_rate_cents: 3941, // $39.41
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Z201A",
        description: "Curette — Additional Lesion",
        ffs_rate_cents: 1970, // $19.70
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E540A",
        description: "Toenail Removal — Under Block",
        ffs_rate_cents: 8867, // $88.67
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "E541A",
        description: "Toenail Wedge Resection with Phenol",
        ffs_rate_cents: 10837, // $108.37
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── eConsult (out-of-basket, 100% FFS) ───────────────────────────────
    OhipCode {
        code: "K998A",
        description: "Physician-to-Physician Telephone Consultation",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K738A",
        description: "eConsult — Specialist Seeking GP Input",
        ffs_rate_cents: 2608, // $26.08
        basket: Basket::Out,
        shadow_pct: 100,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Virtual Care Modality Indicators (required since Dec 2022) ───────
    OhipCode {
        code: "K300A",
        description: "Video Visit Modality Indicator",
        ffs_rate_cents: 0,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "K301A",
        description: "Telephone Visit Modality Indicator",
        ffs_rate_cents: 0,
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    // ── Additional Commonly Billed Codes ─────────────────────────────────
    OhipCode {
        code: "K133A",
        description: "Periodic Health Visit — Adults with IDD",
        ffs_rate_cents: 9135, // $91.35
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Counselling,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "Q888A",
        description: "Weekend Office Access Premium (FHO)",
        ffs_rate_cents: 4455, // $44.55
        basket: Basket::In,
        shadow_pct: 30,
        category: CodeCategory::Premium,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G375A",
        description: "Intralesional Infiltration — 1-2 Lesions",
        ffs_rate_cents: 2250, // $22.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
        after_hours_eligible: false,
        max_per_year: None,
    },
    OhipCode {
        code: "G379A",
        description: "Intravenous Administration — Child/Adult",
        ffs_rate_cents: 2250, // $22.50
        basket: Basket::In,
        shadow_pct: 50,
        category: CodeCategory::Procedure,
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
        codes: &["A001A", "A003A", "A004A", "A007A", "A008A", "A888A"],
        reason: "Only one assessment code per encounter",
    },
    ExclusionGroup {
        name: "Periodic health visits",
        codes: &["K130A", "K131A", "K132A"],
        reason: "One age-band periodic health visit per encounter",
    },
    ExclusionGroup {
        name: "Assessment vs periodic",
        codes: &["A001A", "A003A", "A004A", "A007A", "A008A", "A888A", "K130A", "K131A", "K132A"],
        reason: "Assessment and periodic health visit are mutually exclusive",
    },
    ExclusionGroup {
        name: "Counselling codes",
        codes: &["K005A", "K013A", "K033A"],
        reason: "One counselling code per encounter",
    },
    ExclusionGroup {
        name: "Prenatal codes",
        codes: &["P003A", "P004A", "P005A"],
        reason: "One prenatal assessment type per encounter",
    },
    ExclusionGroup {
        name: "Prenatal vs assessment",
        codes: &["P003A", "P004A", "P005A", "A001A", "A003A", "A004A", "A007A", "A008A", "A888A"],
        reason: "Prenatal assessment replaces standard assessment",
    },
    ExclusionGroup {
        name: "Malignant excision sizes",
        codes: &["R048A", "R051A", "R094A"],
        reason: "One excision size category per lesion",
    },
    ExclusionGroup {
        name: "Benign excision sizes",
        codes: &["Z114A", "Z119A"],
        reason: "One excision size category per lesion",
    },
    ExclusionGroup {
        name: "Laceration repair sizes",
        codes: &["Z154A", "Z160A", "Z176A"],
        reason: "One complexity level per wound",
    },
    ExclusionGroup {
        name: "Cryotherapy single/multiple",
        codes: &["Z108A", "Z110A"],
        reason: "Single vs multiple lesion — pick one",
    },
    ExclusionGroup {
        name: "Electrocoagulation single/multiple",
        codes: &["Z112A", "Z113A"],
        reason: "Single vs multiple lesion — pick one",
    },
    ExclusionGroup {
        name: "Epistaxis treatment",
        codes: &["Z314A", "Z315A"],
        reason: "Cautery vs packing — typically one per encounter",
    },
    ExclusionGroup {
        name: "Direct care time",
        codes: &["Q310", "Q311"],
        reason: "In-office vs remote — one setting per encounter",
    },
    ExclusionGroup {
        name: "Trigger point single/multiple",
        codes: &["G371A", "G372A"],
        reason: "Single vs multiple sites — pick one",
    },
    ExclusionGroup {
        name: "Joint injection size",
        codes: &["Z331A", "Z332A"],
        reason: "Small vs large joint — pick one per joint",
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
    ExclusionGroup {
        name: "K013 standalone",
        codes: &["K013A", "A001A", "A003A", "A004A", "A007A", "A008A"],
        reason: "K013 counselling must be sole purpose of visit — cannot add to assessment",
    },
    ExclusionGroup {
        name: "FHO weekend access",
        codes: &["Q888A", "A888A"],
        reason: "Q888A and A888A cannot be billed same day",
    },
    ExclusionGroup {
        name: "Consultation types",
        codes: &["A005A", "A006A", "A905A"],
        reason: "One consultation type per visit",
    },
    ExclusionGroup {
        name: "B960 visit premiums",
        codes: &["B960A", "B961A", "B962A"],
        reason: "One house call premium type per visit",
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
        // 150 original - 2 expired COVID codes (K082A, K083A) + 7 new
        // (K300A, K301A, K133A, Q888A, G375A, G379A, K015A) = 155
        assert_eq!(all_codes().len(), 155);
    }

    #[test]
    fn test_codes_by_category_assessment() {
        let assessments = codes_by_category(CodeCategory::Assessment);
        // Original 6 in-basket (A001A..A888A) + A900A + 3 out-of-basket prenatal (P003A, P004A, P005A)
        // + 2 consultation (C001A, C002A)
        // + 5 hospital (C003A, C004A, C009A, C010A, C012A) + 2 newborn (H003A, H004A)
        // + 5 LTC (A191A..A195A) + 3 house calls (A901A, A902A, A903A)
        // + 7 prenatal/postnatal (P001A, P002A, P006A, P007A, P008A, P009A, P018A)
        // + 2 geriatric (K655A, K656A) + 3 consultations (A005A, A006A, A905A) = 39
        // (B960A/B961A/B962A moved to Premium category)
        assert_eq!(assessments.len(), 39);
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
        assert_eq!(q053.ffs_rate_cents, 52685); // $526.85
        assert_eq!(q053.basket, Basket::Out);

        let q054 = get_code("Q054A").unwrap();
        assert_eq!(q054.ffs_rate_cents, 36880); // $368.80
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
        let existing = vec!["R048A"]; // small excision
        let conflicts = find_conflicts(&existing, "R051A"); // medium excision
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].reason.contains("excision"));
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
}
