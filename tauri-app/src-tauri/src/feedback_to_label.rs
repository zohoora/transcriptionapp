//! Translate a clinician's `SessionFeedback` into a `LabelData` record for the
//! regression corpus.
//!
//! Context — the label schema already used by `tools/labeled_regression_cli.rs`:
//!
//!   struct LabelData {
//!       split_correct:          Option<bool>,
//!       merge_correct:          Option<bool>,
//!       clinical_correct:       Option<bool>,
//!       patient_count_correct:  Option<bool>,
//!       billing_codes_expected: Option<Vec<String>>,
//!       diagnostic_code_expected: Option<String>,
//!       notes:                  Option<String>,
//!   }
//!
//! `None` on a boolean = "unlabeled", which the regression CLI silently skips.
//! `Some(true)` = locked-in assertion that production was right.
//! `Some(false)` = locked-in assertion that production was wrong.
//!
//! Input (`SessionFeedback`, v2): thumbs-up/down, optional detection category
//! (inappropriately_merged / fragment / wrong_nonclinical / wrong_clinical /
//! other), per-patient content issues (missed_details / inaccurate /
//! wrong_attribution / hallucinated), free-text comments, and the four
//! explicit v2 accuracy booleans (split_correct / merge_correct /
//! clinical_correct / patient_count_correct), each also Option<bool>.

use crate::local_archive::SessionFeedback;
use serde::{Deserialize, Serialize};

/// Canonical label record. Mirrors the on-disk schema in
/// `tests/fixtures/labels/*.json` consumed by `labeled_regression_cli`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct LabelData {
    #[serde(default)]
    pub split_correct: Option<bool>,
    #[serde(default)]
    pub merge_correct: Option<bool>,
    #[serde(default)]
    pub clinical_correct: Option<bool>,
    #[serde(default)]
    pub patient_count_correct: Option<bool>,
    #[serde(default)]
    pub billing_codes_expected: Option<Vec<String>>,
    #[serde(default)]
    pub diagnostic_code_expected: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Translate a session's feedback into a ground-truth label record.
///
/// TODO(clinician): fill this in. Key decisions to make:
///
///   1. Thumbs-up alone — does it imply all four accuracy booleans are
///      Some(true), or should unrated fields stay None?
///
///   2. If `feedback.detection_feedback.category == "inappropriately_merged"`,
///      should that automatically set `merge_correct = Some(false)` — even
///      when the user didn't click the merge accuracy button explicitly?
///      Same question for `"fragment"` → `split_correct = Some(false)`,
///      and `"wrong_nonclinical"` / `"wrong_clinical"` →
///      `clinical_correct = Some(false)`.
///
///   3. Content issues are per-patient. `"wrong_attribution"` on any patient
///      implies patient_count_correct = Some(false)? Or only when there are
///      multiple patients?
///
///   4. The explicit v2 boolean fields (fb.split_correct etc.) are the most
///      trustworthy signal. Should they always win over the category-derived
///      ones, or should conflicting signals downgrade to None (unknown)?
///
/// Billing ground truth: when `fb.billing_correct == Some(true)`, the caller
/// should populate `billing_codes_expected` + `diagnostic_code_expected` from
/// the session's `billing.json` (current confirmed codes). The translate()
/// function here doesn't touch them — it doesn't know the session path — so
/// the call site (e.g. `labeled_regression_cli`) handles that lookup.
/// When `billing_correct == Some(false)`, leave both billing fields None so
/// the regression CLI skips them (user said current codes are wrong, so we
/// can't derive ground truth from them yet).
///
/// Feel free to concatenate `comments` + `detection_feedback.details` into the
/// `notes` field so the regression CLI has human context when a mismatch fires.
pub fn translate(fb: &SessionFeedback) -> LabelData {
    // TODO: replace this body with the logic the decisions above produce.
    // Leaving a neutral default so the module compiles and the call sites
    // (regression CLI, export tool) can be wired up in parallel.
    let _ = fb;
    LabelData::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_archive::SessionFeedback;

    fn empty_fb() -> SessionFeedback {
        SessionFeedback {
            schema_version: 2,
            created_at: "2026-04-23T00:00:00Z".to_string(),
            updated_at: "2026-04-23T00:00:00Z".to_string(),
            quality_rating: None,
            detection_feedback: None,
            patient_feedback: vec![],
            comments: None,
            split_correct: None,
            merge_correct: None,
            clinical_correct: None,
            patient_count_correct: None,
            billing_correct: None,
        }
    }

    #[test]
    fn no_feedback_yields_all_none() {
        let out = translate(&empty_fb());
        assert!(out.split_correct.is_none());
        assert!(out.merge_correct.is_none());
        assert!(out.clinical_correct.is_none());
        assert!(out.patient_count_correct.is_none());
    }

    // TODO(clinician): add tests that pin the decisions made above. Example:
    //
    // #[test]
    // fn thumbs_up_implies_all_correct() {
    //     let mut fb = empty_fb();
    //     fb.quality_rating = Some("good".to_string());
    //     let out = translate(&fb);
    //     assert_eq!(out.split_correct, Some(true));
    //     assert_eq!(out.merge_correct, Some(true));
    //     ...
    // }
}
