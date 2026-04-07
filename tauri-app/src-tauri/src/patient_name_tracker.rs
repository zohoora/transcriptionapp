//! Patient name and DOB extraction and tracking for continuous mode.
//!
//! Uses vision-based screenshot analysis to extract patient names and
//! dates of birth from the screen. A majority-vote tracker accumulates
//! votes across multiple screenshots per encounter to determine the most
//! likely patient name. DOB uses most-recent-wins (no voting needed).

use chrono::Datelike;
use std::collections::HashMap;

/// Tracks patient name votes from periodic screenshot analysis.
/// Uses recency-weighted voting: later screenshots count more than earlier ones,
/// since clinicians often open the patient chart after the encounter starts.
/// The Nth screenshot gets weight N (linear ramp).
pub struct PatientNameTracker {
    /// Name -> recency-weighted vote total (later screenshots count more)
    votes: HashMap<String, u64>,
    /// Incrementing sequence number — next vote gets this + 1 as its weight
    vote_seq: u64,
    /// Last encounter's majority name (set during reset, used for stale screenshot detection)
    previous_name: Option<String>,
    /// Patient date of birth in YYYY-MM-DD format (most recent successful extraction wins)
    dob: Option<String>,
}

impl PatientNameTracker {
    pub fn new() -> Self {
        Self {
            votes: HashMap::new(),
            vote_seq: 0,
            previous_name: None,
            dob: None,
        }
    }

    /// Record a vote for a patient name (normalized: trimmed, title-cased).
    /// Weight increases linearly: 1st screenshot = weight 1, 2nd = weight 2, etc.
    pub fn record(&mut self, name: &str) {
        let normalized = normalize_patient_name(name);
        if !normalized.is_empty() {
            self.vote_seq += 1;
            *self.votes.entry(normalized).or_insert(0) += self.vote_seq;
        }
    }

    /// Returns the name with the highest recency-weighted total, or None if no votes recorded
    pub fn majority_name(&self) -> Option<String> {
        self.votes
            .iter()
            .max_by_key(|(_, weight)| *weight)
            .map(|(name, _)| name.clone())
    }

    /// Returns the total number of screenshots analyzed (not the weighted total)
    pub fn vote_count(&self) -> usize {
        self.vote_seq as usize
    }

    /// Record a vote and check if the majority name changed.
    /// Returns (changed, old_majority, new_majority).
    /// `changed` is true only when both old and new majorities exist and differ.
    pub fn record_and_check_change(&mut self, name: &str) -> (bool, Option<String>, Option<String>) {
        let prev = self.majority_name();
        self.record(name);
        let current = self.majority_name();
        let changed = match (&prev, &current) {
            (Some(old), Some(new)) => old != new,
            _ => false,
        };
        (changed, prev, current)
    }

    /// Clear all votes for a new encounter period, storing outgoing majority name
    pub fn reset(&mut self) {
        self.previous_name = self.majority_name();
        self.votes.clear();
        self.vote_seq = 0;
        self.dob = None;
    }

    /// Store the patient's date of birth (most recent extraction wins, no voting needed).
    pub fn set_dob(&mut self, dob: String) {
        self.dob = Some(dob);
    }

    /// Returns the stored date of birth, if any.
    pub fn dob(&self) -> Option<&str> {
        self.dob.as_deref()
    }

    /// Calculate the patient's age bracket from the stored DOB.
    /// Returns None if no DOB is stored or if the DOB can't be parsed.
    pub fn age_bracket(&self) -> Option<&'static str> {
        let dob = self.dob.as_ref()?;
        let birth = chrono::NaiveDate::parse_from_str(dob, "%Y-%m-%d").ok()?;
        let today = chrono::Local::now().date_naive();
        let age = today.year() - birth.year()
            - if today.ordinal() < birth.ordinal() { 1 } else { 0 };
        Some(match age {
            0..=1 => "child_0_1",
            2..=15 => "child_2_15",
            16..=17 => "adolescent",
            18..=64 => "adult",
            _ => "senior",
        })
    }

    /// Returns the previous encounter's majority name (set during reset)
    pub fn previous_name(&self) -> Option<&str> {
        self.previous_name.as_deref()
    }

    /// Returns a reference to the weighted votes map (for replay bundle snapshots)
    pub fn votes(&self) -> &std::collections::HashMap<String, u64> {
        &self.votes
    }
}

/// Normalize a patient name: handle "Last, First" → "First Last" format,
/// trim whitespace, collapse multiple spaces, title-case.
fn normalize_patient_name(name: &str) -> String {
    // Handle "Surname, Given Middle" → "Given Middle Surname" format
    let reordered = if let Some((before_comma, after_comma)) = name.split_once(',') {
        let surname = before_comma.trim();
        let given = after_comma.trim();
        if !surname.is_empty() && !given.is_empty() {
            format!("{} {}", given, surname)
        } else {
            name.to_string()
        }
    } else {
        name.to_string()
    };

    reordered
        .split_whitespace()
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

/// Build the vision prompt for patient name and DOB extraction.
/// Returns (system_prompt, user_prompt_text).
pub(crate) fn build_patient_name_prompt() -> (String, String) {
    let system = "You are analyzing a screenshot of a computer screen in a clinical setting. \
        If a patient's chart or medical record is visible, extract the patient's full name \
        and date of birth. Respond with ONLY a JSON object, no other text.";

    let user = "Extract patient name and date of birth from this screenshot. \
        Respond with ONLY: {\"name\": \"<full name or NOT_FOUND>\", \"dob\": \"<YYYY-MM-DD or NOT_FOUND>\"}";

    (system.to_string(), user.to_string())
}

/// Parse the vision model's response for a patient name and date of birth.
/// Returns `(Option<name>, Option<dob>)` where DOB is in "YYYY-MM-DD" format.
///
/// Tries JSON parsing first (`{"name": "...", "dob": "..."}`), then falls back
/// to plain-text parsing for backward compatibility.
pub(crate) fn parse_vision_response(response: &str) -> (Option<String>, Option<String>) {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    // Try JSON parsing first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let name = json.get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && !s.contains("NOT_FOUND"))
            .map(|s| normalize_patient_name(s))
            .filter(|s| !s.is_empty());

        let dob = json.get("dob")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && !s.contains("NOT_FOUND"))
            .filter(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok())
            .map(|s| s.to_string());

        return (name, dob);
    }

    // Fallback: plain-text parsing (backward compat)
    (parse_patient_name(trimmed), None)
}

/// Parse the vision model's response for a patient name (legacy wrapper).
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
        tracker.record("John Smith"); // weight 1
        tracker.record("John Smith"); // weight 2
        tracker.record("John Smith"); // weight 3 → total 6
        tracker.record("Jane Doe"); // weight 4 → total 4
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
        assert_eq!(tracker.vote_count(), 0); // sequence resets too
    }

    #[test]
    fn test_patient_name_tracker_normalization() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("  john   SMITH  ");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));
    }

    #[test]
    fn test_comma_format_normalization() {
        // "Surname, Given" and "Given Surname" should normalize to the same string
        let mut tracker = PatientNameTracker::new();
        tracker.record("Zamorano Sanchez, Claudia Marcela"); // weight 1
        tracker.record("Claudia Marcela Zamorano Sanchez"); // weight 2
        // Both should be counted as the same name (total weight 3)
        assert_eq!(
            tracker.majority_name(),
            Some("Claudia Marcela Zamorano Sanchez".to_string())
        );
    }

    #[test]
    fn test_comma_format_no_false_change() {
        // The exact scenario from the clinic: vision returns same name in different formats
        let mut tracker = PatientNameTracker::new();
        let (changed, _, _) = tracker.record_and_check_change("Claudia Marcela Zamorano Sanchez");
        assert!(!changed);
        let (changed, _, _) = tracker.record_and_check_change("Zamorano Sanchez, Claudia Marcela");
        assert!(!changed, "Same name in comma format should NOT trigger a change");
    }

    #[test]
    fn test_recency_weighting_late_chart_open() {
        // Scenario: chart opened at screenshot 5 of 8 (4-min encounter at 30s intervals)
        // Old patient on screen for first 4 screenshots, correct patient for last 4
        let mut tracker = PatientNameTracker::new();
        tracker.record("Wrong Patient"); // weight 1
        tracker.record("Wrong Patient"); // weight 2
        tracker.record("Wrong Patient"); // weight 3
        tracker.record("Wrong Patient"); // weight 4 → total 10
        tracker.record("Correct Patient"); // weight 5
        tracker.record("Correct Patient"); // weight 6
        tracker.record("Correct Patient"); // weight 7
        tracker.record("Correct Patient"); // weight 8 → total 26
        // Correct patient wins despite equal screenshot count (26 vs 10)
        assert_eq!(
            tracker.majority_name(),
            Some("Correct Patient".to_string())
        );
        assert_eq!(tracker.vote_count(), 8);
    }

    #[test]
    fn test_recency_weighting_very_late_chart_open() {
        // Extreme: chart opened at screenshot 7 of 8 — only last 2 screenshots correct
        let mut tracker = PatientNameTracker::new();
        for _ in 0..6 {
            tracker.record("Wrong Patient"); // weights 1+2+3+4+5+6 = 21
        }
        tracker.record("Correct Patient"); // weight 7
        tracker.record("Correct Patient"); // weight 8 → total 15
        // Wrong patient still wins when chart opened very late (21 vs 15)
        // This is expected — 2 screenshots isn't enough to overcome 6
        assert_eq!(tracker.majority_name(), Some("Wrong Patient".to_string()));
    }

    #[test]
    fn test_vote_count_tracks_screenshots() {
        let mut tracker = PatientNameTracker::new();
        assert_eq!(tracker.vote_count(), 0);
        tracker.record("John Smith");
        tracker.record("Jane Doe");
        tracker.record("John Smith");
        assert_eq!(tracker.vote_count(), 3);
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
        // Prompt should now ask for DOB in JSON format
        assert!(system.contains("date of birth"), "system prompt should mention date of birth");
        assert!(user.contains("dob"), "user prompt should mention dob field");
    }

    #[test]
    fn test_reset_stores_previous_name() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        assert_eq!(tracker.previous_name(), None); // No previous before first reset
        tracker.reset();
        assert_eq!(tracker.previous_name(), Some("John Smith"));
        assert_eq!(tracker.majority_name(), None); // Votes cleared
    }

    #[test]
    fn test_previous_name_updates_on_reset() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.reset();
        assert_eq!(tracker.previous_name(), Some("John Smith"));
        tracker.record("Jane Doe");
        tracker.record("Jane Doe");
        tracker.reset();
        assert_eq!(tracker.previous_name(), Some("Jane Doe"));
    }

    #[test]
    fn test_previous_name_none_when_no_votes() {
        let mut tracker = PatientNameTracker::new();
        tracker.reset(); // Reset with no votes
        assert_eq!(tracker.previous_name(), None);
    }

    #[test]
    fn test_record_and_check_change_no_change() {
        let mut tracker = PatientNameTracker::new();
        let (changed, old, new) = tracker.record_and_check_change("John Smith");
        assert!(!changed, "First record should not be a change (no previous majority)");
        assert_eq!(old, None);
        assert_eq!(new, Some("John Smith".to_string()));
    }

    #[test]
    fn test_record_and_check_change_same_name() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        let (changed, old, new) = tracker.record_and_check_change("John Smith");
        assert!(!changed, "Same name should not trigger change");
        assert_eq!(old, Some("John Smith".to_string()));
        assert_eq!(new, Some("John Smith".to_string()));
    }

    #[test]
    fn test_record_and_check_change_new_majority() {
        // Use record_and_check_change for every vote to track exactly when change occurs
        let mut tracker = PatientNameTracker::new();
        // First: establish John as sole majority
        let (changed, _, _) = tracker.record_and_check_change("John Smith");
        assert!(!changed, "First vote can't be a change");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));

        // Strengthen John's majority
        let (changed, _, _) = tracker.record_and_check_change("John Smith");
        assert!(!changed, "Same name shouldn't trigger change");
        // John: weight 1+2 = 3

        // Now add Jane votes — with recency weighting, Jane's later votes carry more weight
        // Jane vote 3: weight 3 → Jane=3, John=3 (tie or flip)
        // Jane vote 4: weight 4 → Jane=7, John=3 (definite flip)
        let mut saw_change = false;
        for _ in 0..5 {
            let (changed, old, new) = tracker.record_and_check_change("Jane Smith");
            if changed {
                saw_change = true;
                assert_eq!(old, Some("John Smith".to_string()));
                assert_eq!(new, Some("Jane Smith".to_string()));
                break;
            }
        }
        assert!(saw_change, "Majority should eventually change from John to Jane");
        assert_eq!(tracker.majority_name(), Some("Jane Smith".to_string()));
    }

    #[test]
    fn test_record_and_check_change_after_reset() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        tracker.reset();
        // After reset, no previous majority
        let (changed, old, new) = tracker.record_and_check_change("Jane Smith");
        assert!(!changed, "After reset, no previous majority to compare against");
        assert_eq!(old, None);
        assert_eq!(new, Some("Jane Smith".to_string()));
    }

    // ── DOB extraction tests ──

    #[test]
    fn test_dob_set_and_get() {
        let mut tracker = PatientNameTracker::new();
        assert_eq!(tracker.dob(), None);
        tracker.set_dob("1990-05-15".to_string());
        assert_eq!(tracker.dob(), Some("1990-05-15"));
    }

    #[test]
    fn test_dob_cleared_on_reset() {
        let mut tracker = PatientNameTracker::new();
        tracker.set_dob("1990-05-15".to_string());
        tracker.reset();
        assert_eq!(tracker.dob(), None);
    }

    #[test]
    fn test_age_bracket_adult() {
        let mut tracker = PatientNameTracker::new();
        tracker.set_dob("1980-01-01".to_string());
        assert_eq!(tracker.age_bracket(), Some("adult"));
    }

    #[test]
    fn test_age_bracket_senior() {
        let mut tracker = PatientNameTracker::new();
        tracker.set_dob("1950-01-01".to_string());
        assert_eq!(tracker.age_bracket(), Some("senior"));
    }

    #[test]
    fn test_age_bracket_child() {
        let mut tracker = PatientNameTracker::new();
        // Use a date that will be a child for a long time
        let today = chrono::Local::now().date_naive();
        let child_dob = today.checked_add_signed(chrono::Duration::days(-365 * 5)).unwrap();
        tracker.set_dob(child_dob.format("%Y-%m-%d").to_string());
        assert_eq!(tracker.age_bracket(), Some("child_2_15"));
    }

    #[test]
    fn test_age_bracket_none_without_dob() {
        let tracker = PatientNameTracker::new();
        assert_eq!(tracker.age_bracket(), None);
    }

    #[test]
    fn test_age_bracket_invalid_dob() {
        let mut tracker = PatientNameTracker::new();
        tracker.set_dob("not-a-date".to_string());
        assert_eq!(tracker.age_bracket(), None);
    }

    // ── parse_vision_response tests ──

    #[test]
    fn test_parse_vision_response_json_both() {
        let (name, dob) = parse_vision_response(
            r#"{"name": "John Smith", "dob": "1985-03-22"}"#
        );
        assert_eq!(name, Some("John Smith".to_string()));
        assert_eq!(dob, Some("1985-03-22".to_string()));
    }

    #[test]
    fn test_parse_vision_response_json_name_only() {
        let (name, dob) = parse_vision_response(
            r#"{"name": "Jane Doe", "dob": "NOT_FOUND"}"#
        );
        assert_eq!(name, Some("Jane Doe".to_string()));
        assert_eq!(dob, None);
    }

    #[test]
    fn test_parse_vision_response_json_dob_only() {
        let (name, dob) = parse_vision_response(
            r#"{"name": "NOT_FOUND", "dob": "1992-11-05"}"#
        );
        assert_eq!(name, None);
        assert_eq!(dob, Some("1992-11-05".to_string()));
    }

    #[test]
    fn test_parse_vision_response_json_both_not_found() {
        let (name, dob) = parse_vision_response(
            r#"{"name": "NOT_FOUND", "dob": "NOT_FOUND"}"#
        );
        assert_eq!(name, None);
        assert_eq!(dob, None);
    }

    #[test]
    fn test_parse_vision_response_json_invalid_dob_format() {
        let (name, dob) = parse_vision_response(
            r#"{"name": "John Smith", "dob": "March 22, 1985"}"#
        );
        assert_eq!(name, Some("John Smith".to_string()));
        assert_eq!(dob, None, "Non-YYYY-MM-DD dates should be rejected");
    }

    #[test]
    fn test_parse_vision_response_plain_text_fallback() {
        let (name, dob) = parse_vision_response("John Smith");
        assert_eq!(name, Some("John Smith".to_string()));
        assert_eq!(dob, None, "Plain text can't contain DOB");
    }

    #[test]
    fn test_parse_vision_response_not_found_plain_text() {
        let (name, dob) = parse_vision_response("NOT_FOUND");
        assert_eq!(name, None);
        assert_eq!(dob, None);
    }

    #[test]
    fn test_parse_vision_response_empty() {
        let (name, dob) = parse_vision_response("");
        assert_eq!(name, None);
        assert_eq!(dob, None);
    }

    #[test]
    fn test_parse_vision_response_json_with_whitespace() {
        let (name, dob) = parse_vision_response(
            r#"  {"name": "John Smith", "dob": "1985-03-22"}  "#
        );
        assert_eq!(name, Some("John Smith".to_string()));
        assert_eq!(dob, Some("1985-03-22".to_string()));
    }

    #[test]
    fn test_parse_vision_response_json_comma_name() {
        // Ensure "Surname, Given" normalization works through JSON path too
        let (name, dob) = parse_vision_response(
            r#"{"name": "Zamorano Sanchez, Claudia", "dob": "1990-01-15"}"#
        );
        assert_eq!(name, Some("Claudia Zamorano Sanchez".to_string()));
        assert_eq!(dob, Some("1990-01-15".to_string()));
    }
}
