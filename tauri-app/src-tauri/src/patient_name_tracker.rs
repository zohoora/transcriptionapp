//! Patient name extraction and tracking for continuous mode.
//!
//! Uses vision-based screenshot analysis to extract patient names from
//! the screen. A majority-vote tracker accumulates votes across multiple
//! screenshots per encounter to determine the most likely patient name.

use std::collections::HashMap;

/// Tracks patient name votes from periodic screenshot analysis.
/// Multiple screenshots are analyzed per encounter; majority vote determines
/// the most likely patient name for labeling.
pub struct PatientNameTracker {
    /// Name -> count of screenshots where this name was extracted
    votes: HashMap<String, u32>,
}

impl PatientNameTracker {
    pub fn new() -> Self {
        Self {
            votes: HashMap::new(),
        }
    }

    /// Record a vote for a patient name (normalized: trimmed, title-cased)
    pub fn record(&mut self, name: &str) {
        let normalized = normalize_patient_name(name);
        if !normalized.is_empty() {
            *self.votes.entry(normalized).or_insert(0) += 1;
        }
    }

    /// Returns the name with the most votes, or None if no votes recorded
    pub fn majority_name(&self) -> Option<String> {
        self.votes
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name.clone())
    }

    /// Clear all votes for a new encounter period
    pub fn reset(&mut self) {
        self.votes.clear();
    }
}

/// Normalize a patient name: trim whitespace, collapse multiple spaces, title-case
fn normalize_patient_name(name: &str) -> String {
    name.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the vision prompt for patient name extraction.
/// Returns (system_prompt, user_prompt_text).
pub(crate) fn build_patient_name_prompt() -> (String, String) {
    let system = "You are analyzing a screenshot of a computer screen in a clinical setting. \
        If a patient's chart or medical record is clearly visible, extract the patient's full name. \
        If no patient name is clearly visible, respond with NOT_FOUND.";

    let user = "Extract the patient name if one is clearly visible on screen. \
        Respond with ONLY the patient name or NOT_FOUND. No explanation.";

    (system.to_string(), user.to_string())
}

/// Parse the vision model's response for a patient name.
/// Returns Some(name) if a name was extracted, None if NOT_FOUND or empty.
pub(crate) fn parse_patient_name(response: &str) -> Option<String> {
    let trimmed = response.trim();
    if trimmed.is_empty() || trimmed.contains("NOT_FOUND") {
        return None;
    }
    let normalized = normalize_patient_name(trimmed);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patient_name_tracker_majority() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        tracker.record("John Smith");
        tracker.record("Jane Doe");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));
    }

    #[test]
    fn test_patient_name_tracker_empty() {
        let tracker = PatientNameTracker::new();
        assert_eq!(tracker.majority_name(), None);
    }

    #[test]
    fn test_patient_name_tracker_reset() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        assert!(tracker.majority_name().is_some());
        tracker.reset();
        assert_eq!(tracker.majority_name(), None);
    }

    #[test]
    fn test_patient_name_tracker_normalization() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("  john   SMITH  ");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));
    }

    #[test]
    fn test_parse_patient_name_found() {
        assert_eq!(
            parse_patient_name("John Smith"),
            Some("John Smith".to_string())
        );
    }

    #[test]
    fn test_parse_patient_name_not_found() {
        assert_eq!(parse_patient_name("NOT_FOUND"), None);
    }

    #[test]
    fn test_parse_patient_name_empty() {
        assert_eq!(parse_patient_name(""), None);
        assert_eq!(parse_patient_name("   "), None);
    }

    #[test]
    fn test_parse_patient_name_whitespace() {
        assert_eq!(
            parse_patient_name("  John Smith  "),
            Some("John Smith".to_string())
        );
    }

    #[test]
    fn test_parse_patient_name_not_found_in_sentence() {
        // If the response contains NOT_FOUND anywhere, treat as not found
        assert_eq!(parse_patient_name("The result is NOT_FOUND here"), None);
    }

    #[test]
    fn test_build_patient_name_prompt() {
        let (system, user) = build_patient_name_prompt();
        assert!(!system.is_empty());
        assert!(!user.is_empty());
        assert!(system.contains("patient"));
        assert!(user.contains("NOT_FOUND"));
    }
}
