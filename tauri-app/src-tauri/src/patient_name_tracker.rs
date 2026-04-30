//! Patient name and DOB extraction and tracking for continuous mode.
//!
//! Uses vision-based screenshot analysis to extract patient names and
//! dates of birth from the screen. A majority-vote tracker accumulates
//! votes across multiple screenshots per encounter to determine the most
//! likely patient name. DOB uses most-recent-wins (no voting needed).

use std::collections::HashMap;

use chrono::{DateTime, Utc};

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
    /// Name of the current consecutive-match streak (Apr 17 2026 — vision early-stop).
    /// Tracks the most recent recorded name so `streak_count` can gate further vision
    /// calls: once we have K consecutive votes for the same name, the screenshot task
    /// stops making LLM calls for this encounter.
    streak_name: Option<String>,
    /// Number of consecutive recorded votes for `streak_name`. Failed vision calls
    /// (no name returned) do NOT reset the streak because `record()` is never called
    /// for them — the tracker only sees successful extractions.
    streak_count: usize,
    /// Total vision LLM calls attempted for this encounter (incremented by the
    /// screenshot task via `note_vision_attempt_at()` BEFORE each call, including
    /// ones that will fail or return empty). Used as a backstop cap so a
    /// pathological encounter with never-stable names doesn't burn unbounded
    /// LLM budget. Reset on `reset()`.
    vision_calls_attempted: usize,
    /// Timestamp of the most recent vision call (set by
    /// `note_vision_attempt_at`). `should_skip_vision` uses this to throttle
    /// re-sampling after early-stop has fired — a chart-switch mid-encounter
    /// (Apr 20 2026 Room 2 Shelley/Richard mislabel) would otherwise lock in
    /// the pre-switch name forever.
    last_vision_call_at: Option<DateTime<Utc>>,
}

impl PatientNameTracker {
    pub fn new() -> Self {
        Self {
            votes: HashMap::new(),
            vote_seq: 0,
            previous_name: None,
            dob: None,
            streak_name: None,
            streak_count: 0,
            vision_calls_attempted: 0,
            last_vision_call_at: None,
        }
    }

    /// Record a vote for a patient name (normalized: trimmed, title-cased).
    /// Weight increases linearly: 1st screenshot = weight 1, 2nd = weight 2, etc.
    pub fn record(&mut self, name: &str) {
        let normalized = normalize_patient_name(name);
        if !normalized.is_empty() {
            self.vote_seq += 1;
            *self.votes.entry(normalized.clone()).or_insert(0) += self.vote_seq;
            // Streak maintenance: if this recorded name matches the current streak,
            // extend it; otherwise start a new streak at this name.
            if self.streak_name.as_deref() == Some(normalized.as_str()) {
                self.streak_count += 1;
            } else {
                self.streak_name = Some(normalized);
                self.streak_count = 1;
            }
        }
    }

    /// Bump the attempted-call counter and stamp the call time. Callers invoke
    /// this BEFORE each vision LLM request so the cap counts failures, parse
    /// errors, and empty responses too — all of which burn LLM budget even
    /// when they don't result in a recorded vote. The timestamp feeds
    /// `should_skip_vision`'s re-sample cadence.
    pub fn note_vision_attempt_at(&mut self, now: DateTime<Utc>) {
        self.vision_calls_attempted += 1;
        self.last_vision_call_at = Some(now);
    }

    /// Test-only shim: bumps the counter without touching the timestamp.
    /// Production callers must use `note_vision_attempt_at` so re-sample
    /// throttling works.
    #[cfg(test)]
    fn note_vision_attempt(&mut self) {
        self.vision_calls_attempted += 1;
    }

    /// Current length of the consecutive-match streak (read-only, for logging).
    pub fn streak_count(&self) -> usize {
        self.streak_count
    }

    /// Total vision LLM calls attempted for this encounter (read-only).
    pub fn vision_calls_attempted(&self) -> usize {
        self.vision_calls_attempted
    }

    /// Should the screenshot task skip the next vision LLM call?
    ///
    /// Early-stop fires when either:
    ///   • `streak_count >= streak_k`: K consecutive recorded votes for the
    ///     same name, so the tracker has high confidence in the majority.
    ///   • `vision_calls_attempted >= cap`: pathological fallback — an
    ///     encounter that keeps flipping between names would otherwise burn
    ///     unbounded LLM budget.
    ///
    /// Once early-stop fires, calls are throttled rather than skipped
    /// outright: if the most recent call is older than
    /// `re_sample_interval_secs`, one additional call is allowed through so a
    /// chart switch mid-encounter can re-open the voting (Apr 20 2026 Room 2
    /// Shelley/Richard mislabel).
    ///
    /// Screenshots are still captured and archived either way — only the
    /// vision LLM call is gated. Calibrated from Apr 16 2026: `streak_k=5,
    /// cap=30` cut vision calls by ~78% on a stable clinic day; the
    /// `re_sample_interval` adds back periodic chart-change detection.
    pub fn should_skip_vision(
        &self,
        streak_k: usize,
        cap: usize,
        now: DateTime<Utc>,
        re_sample_interval_secs: u64,
    ) -> bool {
        let early_stop_fired =
            self.streak_count >= streak_k || self.vision_calls_attempted >= cap;
        if !early_stop_fired {
            return false;
        }
        match self.last_vision_call_at {
            Some(last) => (now - last).num_seconds() < re_sample_interval_secs as i64,
            None => false,
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

    /// Clear all votes for a new encounter period, storing outgoing majority name.
    /// Also clears the streak + attempt counters so vision early-stop starts fresh
    /// for the next encounter.
    pub fn reset(&mut self) {
        self.previous_name = self.majority_name();
        self.votes.clear();
        self.vote_seq = 0;
        self.dob = None;
        self.streak_name = None;
        self.streak_count = 0;
        self.vision_calls_attempted = 0;
        self.last_vision_call_at = None;
    }

    /// If `new_dob` is Some and differs from the previously-stored DOB, clear
    /// name votes + streak + majority (treat as an EMR chart switch mid-
    /// encounter). Returns `true` iff the mismatch fired.
    ///
    /// `vision_calls_attempted` and `last_vision_call_at` are intentionally
    /// preserved so the per-encounter cap still bounds LLM budget across the
    /// invalidation. The caller is responsible for updating `dob` via
    /// `set_dob()` after checking mismatch — this method deliberately leaves
    /// the DOB field alone so tests can assert the pre/post invalidation
    /// state.
    pub fn invalidate_on_dob_mismatch(&mut self, new_dob: Option<&str>) -> bool {
        let mismatch = matches!(
            (self.dob.as_deref(), new_dob),
            (Some(old), Some(new)) if old != new
        );
        if mismatch {
            self.previous_name = self.majority_name();
            self.votes.clear();
            self.vote_seq = 0;
            self.streak_name = None;
            self.streak_count = 0;
        }
        mismatch
    }

    /// Store the patient's date of birth (most recent extraction wins, no voting needed).
    pub fn set_dob(&mut self, dob: String) {
        self.dob = Some(dob);
    }

    /// Returns the stored date of birth, if any.
    pub fn dob(&self) -> Option<&str> {
        self.dob.as_deref()
    }

    // Age bracket calculation is done in the frontend (BillingTab.tsx)
    // using month/day comparison which correctly handles leap years.

    /// Returns the previous encounter's majority name (set during reset)
    pub fn previous_name(&self) -> Option<&str> {
        self.previous_name.as_deref()
    }

    /// Returns a reference to the weighted votes map (for replay bundle snapshots)
    pub fn votes(&self) -> &std::collections::HashMap<String, u64> {
        &self.votes
    }

    /// Number of distinct patient names ever recorded this encounter.
    /// Returns 0 before any vote, 1+ after.
    pub fn unique_name_count(&self) -> usize {
        self.votes.len()
    }

    /// Class 3 fix from 2026-04-29 forensic review: when vision OCR sees ≥3
    /// distinct patient names within one encounter, the chart-vs-audio
    /// alignment is suspicious (Sara 2:50pm visit had 4 distinct names —
    /// Shirley → Catherine → Jaden → Sara — because the clinician opened
    /// each chart in turn during the multi-patient block). The recency-
    /// weighted majority picks whichever chart was open last, which is
    /// often NOT the patient whose content dominates the transcript.
    ///
    /// When this returns true, the orchestrator should treat the
    /// vision-derived name as low-confidence and surface a "verify patient"
    /// flag in the UI rather than auto-applying the recency-weighted
    /// majority. The threshold (3) is conservative — most stable encounters
    /// have ≤2 unique names due to OCR noise on a single chart (e.g.
    /// "Devogela" / "Devoege" / "Deveuge" all referring to the same
    /// Catherine Deveuge — see Catherine 2:35 today).
    pub fn is_chart_likely_stale(&self) -> bool {
        self.unique_name_count() >= 3
    }
}

/// Class 3 fix from 2026-04-29 forensic review (Catherine 2:35 was actually
/// Shirley Rice — vision saw the leftover Catherine Deveuge chart for 12 of
/// 15 minutes; vision early-stop locked in the wrong name).
///
/// Extract candidate patient names from greeting patterns at the START of a
/// clinical transcript. The doctor's first turn typically establishes the
/// patient's identity in one of:
///   - "Hi, [Name]" / "Hello [Name]" / "Good morning [Name]"
///   - "Mr./Mrs./Ms./Mr/Mrs/Ms [Name]"
///   - "[Name], how are you?" / "[Name], please come in"
///
/// Returns ALL distinct candidates found within the first `prefix_words`
/// words of the transcript (default 200). The cross-check consumer compares
/// vision-derived names against these candidates: when no candidate matches,
/// vision confidence drops and the UI surfaces a "verify patient" prompt.
///
/// The parser is conservative — it favors PRECISION over recall (false
/// positives would actively mislead vision cross-check). Common false-positive
/// patterns we explicitly skip: doctor self-introduction ("Hi, this is Dr X"),
/// generic salutations ("hi there", "hello everyone"), and medical jargon
/// that happens to be capitalized (HbA1c, EKG).
pub fn extract_greeting_name_candidates(transcript: &str, prefix_words: usize) -> Vec<String> {
    let words: Vec<&str> = transcript.split_whitespace().take(prefix_words).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let prefix = words.join(" ");
    let lower = prefix.to_lowercase();

    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Triggers that introduce a patient name. Each trigger maps to the
    // expected position of the name relative to the trigger end.
    const TRIGGERS: &[&str] = &[
        "hi,", "hi ", "hello,", "hello ", "hey,", "hey ",
        "good morning,", "good morning ",
        "good afternoon,", "good afternoon ",
        "good evening,", "good evening ",
        "mr.", "mr ", "mrs.", "mrs ", "ms.", "ms ", "miss ",
    ];

    // Doctor-self-intro patterns we MUST skip because they introduce the
    // doctor, not the patient. Anchored to the start of the trigger match.
    const DOCTOR_SELF_INTRO_TAILS: &[&str] = &[
        "this is dr",
        "this is doctor",
        "i'm dr",
        "i am dr",
        "doctor ",  // "Hi, Doctor Smith" — refers to a colleague being addressed
        "dr.",
        "dr ",
    ];

    for trigger in TRIGGERS {
        let mut from = 0;
        while let Some(idx) = lower[from..].find(trigger) {
            let abs = from + idx;
            from = abs + trigger.len();
            // Skip doctor-self-intro patterns
            let after = &lower[abs + trigger.len()..];
            let after_trim = after.trim_start();
            if DOCTOR_SELF_INTRO_TAILS.iter().any(|t| after_trim.starts_with(t)) {
                continue;
            }
            // Greeting-only ("hi there", "hello everyone", "hi how are you")
            const GREETING_ONLY_TAILS: &[&str] = &[
                "there", "everyone", "everybody", "all", "guys", "folks",
                "how", "what", "good", "doc", "doctor",
            ];
            let next_word = after_trim.split_whitespace().next().unwrap_or("");
            if GREETING_ONLY_TAILS.contains(&next_word) {
                continue;
            }
            // Capture 1-3 capitalized words from the original-case prefix at the
            // matching position. We use the original case to gate on capitalization.
            let candidate = capture_proper_name_at(&prefix, abs + trigger.len());
            if let Some(name) = candidate {
                let key = name.to_lowercase();
                if seen.insert(key) {
                    out.push(name);
                }
            }
        }
    }

    out
}

/// Capture 1-3 consecutive Capitalized words starting at `byte_offset` in
/// `text`. Skips leading whitespace and punctuation. Stops at lowercase
/// words, punctuation, or end of input. Returns `None` if no Capitalized
/// word starts at the offset.
fn capture_proper_name_at(text: &str, byte_offset: usize) -> Option<String> {
    let tail = text.get(byte_offset..)?;
    let trimmed = tail.trim_start_matches(|c: char| c.is_whitespace() || c == ',' || c == '.');
    let mut words = Vec::new();
    for w in trimmed.split_whitespace().take(4) {
        let cleaned = w.trim_matches(|c: char| c == ',' || c == '.' || c == '?' || c == '!');
        if cleaned.is_empty() {
            break;
        }
        // First char must be uppercase ASCII letter (handles most English names;
        // intentionally narrow to avoid HbA1c / abbreviation false positives).
        let first = cleaned.chars().next()?;
        if !first.is_ascii_uppercase() {
            break;
        }
        // Reject all-caps tokens (likely abbreviations: EKG, HbA1c, etc.).
        if cleaned.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_ascii_uppercase()) {
            break;
        }
        words.push(cleaned.to_string());
        if words.len() >= 3 {
            break;
        }
    }
    if words.is_empty() {
        return None;
    }
    Some(words.join(" "))
}

/// Class 3 cross-check: does the vision-derived majority name plausibly
/// match any audio-derived greeting candidate? Returns `Match`,
/// `Mismatch`, or `Inconclusive` (no audio candidates available).
///
/// The match is a substring or token-overlap comparison (case-insensitive,
/// fuzzy on token boundaries) so partial OCR (e.g. vision "Catherine Ann
/// Deveuge" vs audio "Hi Cathy") can still align via the shared
/// "Catherine"/"Cathy" anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameCrossCheck {
    Match,
    Mismatch,
    Inconclusive,
}

/// Common English-name nicknames mapped to their formal counterparts.
/// `cross_check_vision_vs_audio` uses this to bridge "Cathy" (audio
/// greeting) ↔ "Catherine" (EMR chart). Lowercased; either order matches.
const NICKNAMES: &[(&str, &str)] = &[
    ("cathy", "catherine"), ("kate", "katherine"), ("kathy", "katherine"),
    ("liz", "elizabeth"), ("beth", "elizabeth"),
    ("bob", "robert"), ("rob", "robert"),
    ("bill", "william"), ("will", "william"),
    ("mike", "michael"),
    ("jim", "james"), ("jimmy", "james"),
    ("dave", "david"),
    ("steve", "steven"), ("steve", "stephen"),
    ("rick", "richard"), ("dick", "richard"), ("rich", "richard"),
    ("dan", "daniel"), ("danny", "daniel"),
    ("chris", "christopher"),
    ("matt", "matthew"),
    ("nick", "nicholas"),
    ("tony", "anthony"),
    ("tom", "thomas"),
    ("jen", "jennifer"), ("jenny", "jennifer"),
    ("sam", "samuel"), ("sammy", "samuel"),
    ("alex", "alexander"), ("alex", "alexandra"),
    ("ben", "benjamin"),
    ("pat", "patrick"), ("pat", "patricia"),
];

pub fn cross_check_vision_vs_audio(
    vision_name: Option<&str>,
    audio_candidates: &[String],
) -> NameCrossCheck {
    let Some(vn) = vision_name else { return NameCrossCheck::Inconclusive };
    if vn.trim().is_empty() {
        return NameCrossCheck::Inconclusive;
    }
    if audio_candidates.is_empty() {
        return NameCrossCheck::Inconclusive;
    }
    let vn_lower = vn.to_lowercase();
    let vn_tokens: std::collections::HashSet<&str> = vn_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .collect();
    if vn_tokens.is_empty() {
        return NameCrossCheck::Inconclusive;
    }
    for cand in audio_candidates {
        let cand_lower = cand.to_lowercase();
        let cand_tokens: std::collections::HashSet<&str> = cand_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 3)
            .collect();
        if vn_tokens.intersection(&cand_tokens).next().is_some() {
            return NameCrossCheck::Match;
        }
        for (nick, full) in NICKNAMES {
            if (vn_lower.contains(nick) && cand_lower.contains(full))
                || (vn_lower.contains(full) && cand_lower.contains(nick))
            {
                return NameCrossCheck::Match;
            }
        }
    }
    NameCrossCheck::Mismatch
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
/// When `templates` is provided and the relevant field is non-empty, it overrides the hardcoded default.
pub(crate) fn build_patient_name_prompt(
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let system = templates
        .and_then(|t| (!t.patient_name_system.is_empty()).then(|| t.patient_name_system.clone()))
        .unwrap_or_else(|| "You are analyzing a screenshot of a computer screen in a clinical setting. \
            If a patient's chart or medical record is visible, extract the patient's full name \
            and date of birth. Respond with ONLY a JSON object, no other text.".to_string());

    let user = templates
        .and_then(|t| (!t.patient_name_user.is_empty()).then(|| t.patient_name_user.clone()))
        .unwrap_or_else(|| "Extract patient name and date of birth from this screenshot. \
            Respond with ONLY: {\"name\": \"<full name or NOT_FOUND>\", \"dob\": \"<YYYY-MM-DD or NOT_FOUND>\"}".to_string());

    (system, user)
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

    // LLMs sometimes wrap JSON in markdown code fences (```json ... ```) or
    // return multiple JSON blocks concatenated. Extract the first balanced
    // {...} block and parse that. Falls back to whole-string parse for the
    // common case where the LLM returns clean JSON.
    //
    // Apr 20 2026 fix: Room 6 encounter #2 today produced a response where
    // two JSON objects were concatenated with markdown fences between them;
    // the old serde_json::from_str(whole_response) failed, and the
    // plain-text fallback dumped the entire mangled blob into patient_name
    // (e.g. `"dob": "1945-04-08" } ``` ```json { "name": "...`). Scanning
    // for the first balanced JSON object recovers cleanly.
    let first_json = extract_first_json_object(trimmed).unwrap_or(trimmed);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(first_json) {
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

    // Fallback: plain-text parsing (backward compat, rare path now)
    (parse_patient_name(trimmed), None)
}

/// Find the first balanced `{...}` block in `s`, respecting string escaping.
///
/// Handles markdown-wrapped JSON, leading garbage, and multi-block responses.
/// Returns the exact byte slice of the first complete object (including the
/// outer braces), or None if no balanced object is found.
fn extract_first_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for (idx, &b) in bytes[start..].iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + idx + 1;
                    return Some(&s[start..end]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a plain-text vision response for a patient name.
/// Internal helper used as fallback by `parse_vision_response`.
fn parse_patient_name(response: &str) -> Option<String> {
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

    // ============================================================
    // Class 3 fix tests — chart-stuck cross-check (2026-04-29)
    // ============================================================

    #[test]
    fn unique_name_count_starts_zero() {
        let t = PatientNameTracker::new();
        assert_eq!(t.unique_name_count(), 0);
        assert!(!t.is_chart_likely_stale());
    }

    #[test]
    fn one_or_two_distinct_names_not_stale() {
        // Catherine 2:35: vision OCR returned "Devogela", "Devoege",
        // "Deveuge" — these normalize differently but are <3 unique. Single
        // chart with OCR noise should NOT trigger the stale flag.
        let mut t = PatientNameTracker::new();
        t.record("Catherine Devoege");
        t.record("Catherine Devoege");
        assert_eq!(t.unique_name_count(), 1);
        assert!(!t.is_chart_likely_stale());
        t.record("Catherine Deveuge");
        assert_eq!(t.unique_name_count(), 2);
        assert!(!t.is_chart_likely_stale());
    }

    #[test]
    fn three_or_more_distinct_names_flagged_stale() {
        // Sara 2:50pm visit had Shirley → Catherine → Jaden → Sara (4
        // unique names). Must trigger stale flag.
        let mut t = PatientNameTracker::new();
        t.record("Shirley Joanne Rice");
        t.record("Catherine Ann Devouge");
        t.record("Jaden Brandon Slote");
        assert_eq!(t.unique_name_count(), 3);
        assert!(t.is_chart_likely_stale());
        t.record("Sara Izabela Slote");
        assert_eq!(t.unique_name_count(), 4);
        assert!(t.is_chart_likely_stale());
    }

    #[test]
    fn extract_greeting_hi_first_name() {
        let names = extract_greeting_name_candidates("Hi, Shirley. How are you today?", 50);
        assert!(names.iter().any(|n| n.contains("Shirley")), "got: {:?}", names);
    }

    #[test]
    fn extract_greeting_good_morning() {
        let names = extract_greeting_name_candidates("Good morning, Mrs Catherine Deveuge", 50);
        assert!(
            names.iter().any(|n| n.contains("Catherine")),
            "expected Catherine in {:?}", names
        );
    }

    #[test]
    fn extract_greeting_skips_doctor_self_intro() {
        // Janice's actual transcript opener — must NOT capture "Doctor Zohor"
        // as the patient name.
        let names = extract_greeting_name_candidates(
            "Hi, Diana. This is Doctor Zohor Kalin.",
            50,
        );
        assert!(
            names.iter().any(|n| n.contains("Diana")),
            "should capture Diana as patient: {:?}", names
        );
        assert!(
            !names.iter().any(|n| n.to_lowercase().contains("zohor")
                || n.to_lowercase().contains("doctor")),
            "must NOT capture doctor self-intro: {:?}", names
        );
    }

    #[test]
    fn extract_greeting_skips_generic_salutations() {
        let names = extract_greeting_name_candidates(
            "Hi there, how are you? Hello everyone, good morning all.",
            50,
        );
        assert!(names.is_empty(), "generic salutations should not capture: {:?}", names);
    }

    #[test]
    fn extract_greeting_handles_mr_mrs_ms() {
        let names = extract_greeting_name_candidates("Mr Smith, please come in.", 50);
        assert!(
            names.iter().any(|n| n.contains("Smith")),
            "got: {:?}", names
        );
    }

    #[test]
    fn extract_greeting_only_first_n_words() {
        // "Hi Shirley" appears 100 words in — should NOT be picked up if
        // prefix_words=50 (out of range).
        let mut s = "word ".repeat(100);
        s.push_str("Hi, Shirley. How are you?");
        let names = extract_greeting_name_candidates(&s, 50);
        assert!(names.is_empty(), "should not search past prefix_words");
    }

    #[test]
    fn cross_check_match_simple() {
        let candidates = vec!["Shirley".to_string()];
        assert_eq!(
            cross_check_vision_vs_audio(Some("Shirley Joanne Rice"), &candidates),
            NameCrossCheck::Match
        );
    }

    #[test]
    fn cross_check_match_via_nickname() {
        // Vision: "Catherine Deveuge" — audio: "Hi Cathy" — must match
        // via Cathy↔Catherine nickname pair.
        let candidates = vec!["Cathy".to_string()];
        assert_eq!(
            cross_check_vision_vs_audio(Some("Catherine Ann Deveuge"), &candidates),
            NameCrossCheck::Match
        );
    }

    #[test]
    fn cross_check_mismatch_catherine_to_shirley() {
        // Catherine 2:35 — vision said Catherine, audio said Shirley:
        // must report Mismatch so the orchestrator can flag chart-stale.
        let candidates = vec!["Shirley".to_string()];
        assert_eq!(
            cross_check_vision_vs_audio(Some("Catherine Ann Deveuge"), &candidates),
            NameCrossCheck::Mismatch
        );
    }

    #[test]
    fn cross_check_inconclusive_when_no_audio() {
        let candidates: Vec<String> = vec![];
        assert_eq!(
            cross_check_vision_vs_audio(Some("Catherine Ann Deveuge"), &candidates),
            NameCrossCheck::Inconclusive
        );
    }

    #[test]
    fn cross_check_inconclusive_when_no_vision() {
        let candidates = vec!["Shirley".to_string()];
        assert_eq!(
            cross_check_vision_vs_audio(None, &candidates),
            NameCrossCheck::Inconclusive
        );
    }

    // ============================================================

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
        let (system, user) = build_patient_name_prompt(None);
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

    // ── Vision early-stop tests (Apr 17 2026) ──────────────────────────

    #[test]
    fn streak_increments_on_consecutive_matching_votes() {
        let mut t = PatientNameTracker::new();
        t.record("Pani"); assert_eq!(t.streak_count(), 1);
        t.record("Pani"); assert_eq!(t.streak_count(), 2);
        t.record("Pani"); assert_eq!(t.streak_count(), 3);
        t.record("Brown"); assert_eq!(t.streak_count(), 1); // reset to new name
        t.record("Brown"); assert_eq!(t.streak_count(), 2);
    }

    #[test]
    fn streak_not_affected_by_attempts_without_records() {
        // Failed vision calls bump vision_calls_attempted but don't call record().
        // Streak should only track successful records.
        let mut t = PatientNameTracker::new();
        t.record("Pani");
        t.note_vision_attempt(); // simulates a failed call
        t.record("Pani");
        assert_eq!(t.streak_count(), 2, "streak preserved across failed attempts");
    }

    /// Helper: drive one "successful vision call" — stamp the call time and
    /// record a name. Mirrors production semantics in screenshot_task.rs.
    fn record_with_attempt(t: &mut PatientNameTracker, name: &str, at: DateTime<Utc>) {
        t.note_vision_attempt_at(at);
        t.record(name);
    }

    #[test]
    fn should_skip_vision_fires_at_streak_threshold() {
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        for _ in 0..4 {
            record_with_attempt(&mut t, "Pani", now);
            assert!(!t.should_skip_vision(5, 30, now, 600), "not yet at streak=5");
        }
        record_with_attempt(&mut t, "Pani", now);
        assert!(t.should_skip_vision(5, 30, now, 600), "streak=5 triggers skip");
    }

    #[test]
    fn should_skip_vision_fires_at_cap_regardless_of_streak() {
        // Worst case: names keep flipping so streak never reaches K, but we
        // still stop at the cap so LLM budget is bounded.
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        for i in 0..30 {
            t.note_vision_attempt_at(now);
            t.record(if i % 2 == 0 { "A" } else { "B" });
        }
        assert_eq!(t.streak_count(), 1);
        assert!(t.should_skip_vision(5, 30, now, 600), "cap=30 reached");
    }

    #[test]
    fn reset_clears_streak_and_attempts() {
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        record_with_attempt(&mut t, "Pani", now);
        record_with_attempt(&mut t, "Pani", now);
        record_with_attempt(&mut t, "Pani", now);
        t.reset();
        assert_eq!(t.streak_count(), 0);
        assert_eq!(t.vision_calls_attempted(), 0);
        assert!(!t.should_skip_vision(5, 30, now, 600));
    }

    #[test]
    fn should_skip_vision_uses_or_logic() {
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        for _ in 0..5 { record_with_attempt(&mut t, "Pani", now); }
        assert!(
            t.should_skip_vision(5, 100, now, 600),
            "streak branch fires even with cap far away"
        );
    }

    // ── Re-sample throttle + DOB invalidation (v0.10.45) ──

    #[test]
    fn should_skip_vision_re_samples_after_interval() {
        // Early-stop fires after K=5 matching votes. 10 minutes later (> the
        // 600s re-sample interval), the gate should OPEN to allow one more
        // call — this is the Apr 20 Room 2 Shelley fix.
        let mut t = PatientNameTracker::new();
        let start = Utc::now();
        for _ in 0..5 { record_with_attempt(&mut t, "Richard", start); }
        assert!(t.should_skip_vision(5, 30, start, 600), "locks immediately after streak");

        let later = start + chrono::Duration::seconds(599);
        assert!(t.should_skip_vision(5, 30, later, 600), "still locked at 599s");

        let way_later = start + chrono::Duration::seconds(700);
        assert!(
            !t.should_skip_vision(5, 30, way_later, 600),
            "re-sample gate opens past interval"
        );
    }

    #[test]
    fn should_skip_vision_does_not_skip_before_early_stop() {
        // Throttle only applies AFTER early-stop fires. Below the threshold,
        // every call goes through regardless of how recent the last one was.
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        record_with_attempt(&mut t, "Pani", now);
        assert!(!t.should_skip_vision(5, 30, now, 600), "no skip below streak threshold");
    }

    #[test]
    fn dob_mismatch_invalidates_votes_and_streak() {
        let mut t = PatientNameTracker::new();
        let now = Utc::now();
        for _ in 0..5 { record_with_attempt(&mut t, "Richard Mallett", now); }
        t.set_dob("1950-01-15".into());
        assert_eq!(t.streak_count(), 5);
        assert_eq!(t.majority_name(), Some("Richard Mallett".to_string()));

        let fired = t.invalidate_on_dob_mismatch(Some("1970-05-19"));
        assert!(fired, "different DOB should trigger invalidation");
        assert_eq!(t.streak_count(), 0, "streak cleared");
        assert_eq!(t.majority_name(), None, "votes cleared");
        assert_eq!(t.previous_name(), Some("Richard Mallett"), "outgoing saved");
        assert_eq!(t.vision_calls_attempted(), 5, "attempt counter preserved");
    }

    #[test]
    fn dob_mismatch_does_not_fire_for_same_dob() {
        let mut t = PatientNameTracker::new();
        record_with_attempt(&mut t, "Richard", Utc::now());
        t.set_dob("1950-01-15".into());
        assert!(!t.invalidate_on_dob_mismatch(Some("1950-01-15")), "same DOB: no-op");
        assert_eq!(t.streak_count(), 1, "streak preserved");
    }

    #[test]
    fn dob_mismatch_does_not_fire_for_none_or_first_read() {
        let mut t = PatientNameTracker::new();
        record_with_attempt(&mut t, "Shelley", Utc::now());
        // First DOB ever seen: no previous value, so no mismatch.
        assert!(!t.invalidate_on_dob_mismatch(Some("1970-05-19")));
        t.set_dob("1970-05-19".into());
        // None on a later call is ambiguous — don't invalidate.
        assert!(!t.invalidate_on_dob_mismatch(None));
        assert_eq!(t.streak_count(), 1, "streak preserved on None");
    }

    #[test]
    fn streak_matches_normalized_form() {
        // The streak key is the normalized name, so different casings / whitespace
        // of the same patient should still extend a streak.
        let mut t = PatientNameTracker::new();
        t.record("John Smith");
        t.record("JOHN SMITH");
        t.record("john smith");
        assert_eq!(t.streak_count(), 3, "case-insensitive match extends streak");
    }

    // ── extract_first_json_object + markdown-wrapped vision responses ──

    #[test]
    fn extract_first_json_balanced_braces() {
        let s = r#"{"a": 1, "b": {"c": 2}}"#;
        assert_eq!(extract_first_json_object(s), Some(s));
    }

    #[test]
    fn extract_first_json_leading_garbage() {
        let s = r#"here is the json: {"name": "Jane Doe"}"#;
        assert_eq!(
            extract_first_json_object(s),
            Some(r#"{"name": "Jane Doe"}"#)
        );
    }

    #[test]
    fn extract_first_json_markdown_fence() {
        let s = "```json\n{\"name\": \"Jane Doe\", \"dob\": \"1990-01-01\"}\n```";
        let got = extract_first_json_object(s).expect("found object");
        let parsed: serde_json::Value = serde_json::from_str(got).expect("parses");
        assert_eq!(parsed["name"], "Jane Doe");
    }

    #[test]
    fn extract_first_json_two_blocks_returns_first() {
        // Real-world Room 6 2026-04-20 encounter #2 shape: two JSON blocks
        // concatenated between markdown fences.
        let s = r#"```json
{"name": "Judie Joan Guest", "dob": "1945-04-08"}
```

```json
{"name": "judie Joan Guest", "dob": "1945-04-08"}
```"#;
        let got = extract_first_json_object(s).expect("found first object");
        let parsed: serde_json::Value = serde_json::from_str(got).expect("parses");
        assert_eq!(parsed["name"], "Judie Joan Guest");
        assert_eq!(parsed["dob"], "1945-04-08");
    }

    #[test]
    fn extract_first_json_handles_string_with_braces() {
        // Escaped braces inside a string value should not prematurely close.
        let s = r#"{"name": "Weird { Name }", "dob": "1990-01-01"}"#;
        assert_eq!(extract_first_json_object(s), Some(s));
    }

    #[test]
    fn extract_first_json_returns_none_on_unbalanced() {
        assert_eq!(extract_first_json_object("{"), None);
        assert_eq!(extract_first_json_object("{ \"a\": 1 "), None);
        assert_eq!(extract_first_json_object("no braces here"), None);
    }

    #[test]
    fn parse_vision_response_recovers_from_markdown_wrapped_json() {
        // Repro of the Apr 20 2026 Room 6 encounter #2 bug:
        // Old parser dumped the whole mangled string into patient_name.
        // New parser finds the first balanced block and parses it.
        let response = r#"```json
{"name": "Judie Joan Guest", "dob": "1945-04-08"}
```

```json
{"name": "judie Joan Guest", "dob": "1945-04-08"}
```"#;
        let (name, dob) = parse_vision_response(response);
        assert_eq!(name, Some("Judie Joan Guest".to_string()));
        assert_eq!(dob, Some("1945-04-08".to_string()));
    }

    #[test]
    fn parse_vision_response_still_handles_clean_json() {
        let (name, dob) = parse_vision_response(r#"{"name":"Jane Doe","dob":"1990-01-01"}"#);
        assert_eq!(name, Some("Jane Doe".to_string()));
        assert_eq!(dob, Some("1990-01-01".to_string()));
    }

    #[test]
    fn parse_vision_response_not_found_still_returns_none() {
        let (name, dob) = parse_vision_response(r#"{"name":"NOT_FOUND","dob":"NOT_FOUND"}"#);
        assert_eq!(name, None);
        assert_eq!(dob, None);
    }
}
