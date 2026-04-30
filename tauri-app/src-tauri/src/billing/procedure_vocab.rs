//! Canonical "billable procedure" vocabulary shared between:
//!   - `llm_client.rs` SOAP post-filter (drops procedure[] entries that
//!     don't match any billable verb, preventing the v0.10.61
//!     overcapture failure mode where the LLM would list "reviewed blood
//!     work" / "Performed chest auscultation" / "Completed DTC form" as
//!     procedures);
//!   - `tools/labeled_regression_cli.rs` procedure_section_correct check.
//!
//! Both must use the SAME vocabulary or they'll drift; defining it here
//! once with a parity test keeps them aligned.
//!
//! 2026-04-29 forensic review motivated this — 5 sessions had hallucinated
//! procedure entries (Ruth, Allan, Angelika, Catherine 2:35, plus partial
//! transcript-quote leak in Irene). All 5 fail the verb-match check
//! defined here.

/// Verbs / nouns that anchor a billable procedure. Mirrors the canonical
/// `ProcedureType` enum in `clinical_features.rs` — every entry there
/// should map to at least one verb here. Match is case-insensitive
/// substring containment, so partial verb stems ("inject" matches
/// "injected", "injection", "injecting") are intentional.
///
/// When you add a `ProcedureType` variant, also add its anchor verb here
/// and bump the parity test.
pub const BILLABLE_PROCEDURE_VERBS: &[&str] = &[
    // Injections
    "inject", "injection", "injected", "injecting",
    // Joint / trigger / nerve block
    "joint injection", "trigger point", "nerve block", "intralesional",
    "intramuscular", "intravenous", "subcutaneous", "intradermal",
    // Pap / IUD
    "pap smear", "pap test", "iud insert", "iud remov",
    // Skin / lesion
    "biopsy", "biopsied", "biopsying",
    "excis",                            // excise / excision / excised
    "remov",                            // removal / removed (often paired with lesion / nail / FB)
    "extract",                          // extraction (toenail, foreign body)
    "avuls",                            // avulsion (nail)
    "debrid",                           // debridement
    // Cryo / cautery / electrocoag
    "cryotherap", "freezing", "froze", "freezed", "liquid nitrogen",
    "cauter",                           // cauterize / cauterization
    "burn",                             // burned (skin lesion)
    "fulgurat",                         // fulguration
    "electrocoag", "electrocaut",
    // Suture / wound
    "suture", "stitch", "stapl",
    "incision", "drain", "drainage", "drained",
    "abscess", "i&d", "i & d",
    // Foreign body / aspiration
    "foreign body", "fb removal",
    "aspirat", "thoracentesis", "paracentesis",
    "punch", "shave",
    // Ear / nose
    "syring", "irrigat",                // ear syringing / irrigation
    "cerumen", "wax",                   // cerumen removal / earwax
    "epistaxis",
    // Office endoscopy / pressure
    "anoscopy", "sigmoidoscopy", "tonometry",
    // Vaccinations
    "immuniz", "vaccin", "tdap", "hpv", "hep b", "mmr", "varicella",
    // Other
    "splint", "cast appl", "reduction of fracture",
    "ear pierc", "circumcision", "vasectomy",
    "skin tag", "cyst remov", "lipoma", "wart",
    "intervention", "infiltrat",
    "pack", "packing",                  // nasal packing
];

/// True if `action` (case-insensitive) contains any canonical billable
/// procedure verb. Empty / whitespace input returns false.
///
/// 2026-04-29 forensic review failure modes that this rejects:
///   * "Performed chest auscultation"               (Ruth)
///   * "reviewed blood work results"                (Allan, Catherine 2:35)
///   * "Performed physical examination of both knees and hips"  (Angelika)
///   * "Provided printed copy of blood work results" (Catherine 2:35)
///   * "Completed medical section of Disability Tax Credit form" (Catherine 2:35)
///
/// And accepts (these were correct procedures the LLM identified):
///   * "performed ultrasound-guided cervical numbing injection" (Irene)
///   * "Liquid nitrogen applied to plantar wart"
///   * "Cortisone injection into right knee performed"
pub fn is_billable_procedure_action(action: &str) -> bool {
    let lower = action.trim().to_lowercase();
    if lower.is_empty() {
        return false;
    }
    BILLABLE_PROCEDURE_VERBS.iter().any(|v| lower.contains(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-04-29 forensic review failure modes — must REJECT
    #[test]
    fn rejects_chest_auscultation() {
        assert!(!is_billable_procedure_action("Performed chest auscultation"));
    }

    #[test]
    fn rejects_reviewing_blood_work() {
        assert!(!is_billable_procedure_action("reviewed blood work results"));
        assert!(!is_billable_procedure_action("Reviewed blood work results and ECG with patient"));
    }

    #[test]
    fn rejects_physical_examination() {
        assert!(!is_billable_procedure_action("Performed physical examination of both knees and hips"));
    }

    #[test]
    fn rejects_providing_copy_of_results() {
        assert!(!is_billable_procedure_action("Provided patient with printed copy of blood work results"));
    }

    #[test]
    fn rejects_completing_form() {
        assert!(!is_billable_procedure_action("Completed medical section of Disability Tax Credit form"));
    }

    #[test]
    fn rejects_empty_action() {
        assert!(!is_billable_procedure_action(""));
        assert!(!is_billable_procedure_action("   "));
    }

    // Real procedures — must ACCEPT
    #[test]
    fn accepts_nerve_block_injection() {
        assert!(is_billable_procedure_action("performed ultrasound-guided cervical numbing injection"));
    }

    #[test]
    fn accepts_cryotherapy() {
        assert!(is_billable_procedure_action("Liquid nitrogen applied to plantar wart"));
        assert!(is_billable_procedure_action("Froze the wart with cryotherapy"));
    }

    #[test]
    fn accepts_joint_injection() {
        assert!(is_billable_procedure_action("Cortisone injection into right knee performed"));
    }

    #[test]
    fn accepts_pap_smear() {
        assert!(is_billable_procedure_action("Pap smear performed with speculum"));
    }

    #[test]
    fn accepts_skin_biopsy() {
        assert!(is_billable_procedure_action("Punch biopsy of suspicious lesion"));
    }

    #[test]
    fn accepts_ear_syringing() {
        assert!(is_billable_procedure_action("Ear syringing for cerumen impaction"));
    }

    #[test]
    fn accepts_vaccination() {
        assert!(is_billable_procedure_action("Tdap immunization administered"));
    }

    #[test]
    fn case_insensitive_matching() {
        assert!(is_billable_procedure_action("PUNCH BIOPSY"));
        assert!(is_billable_procedure_action("punch biopsy"));
        assert!(is_billable_procedure_action("Punch Biopsy"));
    }
}
