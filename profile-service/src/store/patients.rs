//! Cross-session patient index (v0.10.46+).
//!
//! Keyed by `(physician_id, name_normalized, dob)` so repeated confirms for
//! the same clinical identity land on the same record. Persisted to
//! `patients.json` via atomic rename (same pattern as `PhysicianManager`).
//!
//! Normalization MUST match the tauri-side
//! `patient_name_tracker::normalize_patient_name` — the
//! `normalize_patient_name` helper below is a byte-equivalent port with its
//! own parity test (see `normalization_parity_with_tauri_client`).

use crate::error::ApiError;
use crate::types::PatientRecord;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Default)]
struct PatientStoreFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    records: Vec<PatientRecord>,
}

fn default_schema_version() -> u32 {
    1
}

pub struct PatientManager {
    file: PatientStoreFile,
    path: PathBuf,
    /// (physician_id, name_normalized, dob) → index into `file.records`.
    by_key: BTreeMap<(String, String, String), usize>,
    /// (physician_id, patient_id) → index into `file.records`.
    by_id: BTreeMap<(String, String), usize>,
}

impl PatientManager {
    pub fn load(path: PathBuf) -> Result<Self, ApiError> {
        let file = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::Internal(format!("Failed to read patients: {e}")))?;
            serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse patients: {e}")))?
        } else {
            PatientStoreFile {
                schema_version: 1,
                records: Vec::new(),
            }
        };

        let mut by_key = BTreeMap::new();
        let mut by_id = BTreeMap::new();
        for (idx, r) in file.records.iter().enumerate() {
            by_key.insert(
                (r.physician_id.clone(), normalize_patient_name(&r.name), r.dob.clone()),
                idx,
            );
            by_id.insert((r.physician_id.clone(), r.patient_id.clone()), idx);
        }

        info!(count = file.records.len(), "Loaded patient records");
        Ok(Self {
            file,
            path,
            by_key,
            by_id,
        })
    }

    fn save(&self) -> Result<(), ApiError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::Internal(format!("Failed to create directory: {e}")))?;
        }
        let content = serde_json::to_string_pretty(&self.file)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize patients: {e}")))?;
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &temp_path,
                std::fs::Permissions::from_mode(0o600),
            );
        }
        std::fs::rename(&temp_path, &self.path)
            .map_err(|e| ApiError::Internal(format!("Failed to rename: {e}")))?;
        Ok(())
    }

    /// Idempotent confirm. Lookup-or-create. On hit, appends `session_id`
    /// (deduped) and refreshes `medplum_patient_id` when supplied.
    pub fn confirm(
        &mut self,
        physician_id: &str,
        name: &str,
        dob: &str,
        session_id: &str,
        medplum_patient_id: Option<String>,
    ) -> Result<(PatientRecord, bool), ApiError> {
        if physician_id.is_empty() {
            return Err(ApiError::BadRequest("physician_id is empty".into()));
        }
        if !is_iso_date(dob) {
            return Err(ApiError::BadRequest(format!(
                "dob must be YYYY-MM-DD, got {dob}"
            )));
        }
        let normalized = normalize_patient_name(name);
        if normalized.is_empty() {
            return Err(ApiError::BadRequest("name is empty after normalization".into()));
        }
        let now = Utc::now().to_rfc3339();
        let key = (physician_id.to_string(), normalized.clone(), dob.to_string());

        let created = if let Some(&idx) = self.by_key.get(&key) {
            let rec = &mut self.file.records[idx];
            if !rec.session_ids.iter().any(|s| s == session_id) {
                rec.session_ids.push(session_id.to_string());
            }
            if let Some(mpid) = medplum_patient_id {
                // Later writes win — reconciles a UUID fallback to a real Medplum
                // FHIR ID on a subsequent confirm.
                rec.medplum_patient_id = Some(mpid);
            }
            rec.updated_at = now;
            false
        } else {
            let patient_id = medplum_patient_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let rec = PatientRecord {
                patient_id: patient_id.clone(),
                physician_id: physician_id.to_string(),
                name: normalized.clone(),
                dob: dob.to_string(),
                medplum_patient_id,
                session_ids: vec![session_id.to_string()],
                created_at: now.clone(),
                updated_at: now,
            };
            let idx = self.file.records.len();
            self.file.records.push(rec);
            self.by_key.insert(key, idx);
            self.by_id
                .insert((physician_id.to_string(), patient_id), idx);
            true
        };

        if let Err(e) = self.save() {
            warn!(error = %e, "patients.json save failed after confirm");
            return Err(e);
        }

        let idx = *self
            .by_key
            .get(&(
                physician_id.to_string(),
                normalize_patient_name(name),
                dob.to_string(),
            ))
            .expect("just inserted");
        Ok((self.file.records[idx].clone(), created))
    }

    pub fn get_by_name_dob(
        &self,
        physician_id: &str,
        name: &str,
        dob: &str,
    ) -> Option<PatientRecord> {
        let key = (
            physician_id.to_string(),
            normalize_patient_name(name),
            dob.to_string(),
        );
        self.by_key
            .get(&key)
            .map(|&idx| self.file.records[idx].clone())
    }

    pub fn get_by_patient_id(
        &self,
        physician_id: &str,
        patient_id: &str,
    ) -> Option<PatientRecord> {
        self.by_id
            .get(&(physician_id.to_string(), patient_id.to_string()))
            .map(|&idx| self.file.records[idx].clone())
    }

    pub fn list_for_physician(&self, physician_id: &str) -> Vec<PatientRecord> {
        self.file
            .records
            .iter()
            .filter(|r| r.physician_id == physician_id)
            .cloned()
            .collect()
    }

    /// Remove a patient by (physician_id, patient_id). Used by the DELETE
    /// route for admin/cleanup (e.g., removing test artifacts or merging two
    /// PatientRecords that got created for the same person via DOB typo).
    /// Returns Ok(true) if a record was removed, Ok(false) if not found.
    pub fn delete(
        &mut self,
        physician_id: &str,
        patient_id: &str,
    ) -> Result<bool, ApiError> {
        let Some(&idx) = self
            .by_id
            .get(&(physician_id.to_string(), patient_id.to_string()))
        else {
            return Ok(false);
        };
        let record = self.file.records.remove(idx);
        // Rebuild indices — indices held Vec indexes which all shift after remove.
        self.by_key.clear();
        self.by_id.clear();
        for (new_idx, r) in self.file.records.iter().enumerate() {
            self.by_key.insert(
                (r.physician_id.clone(), normalize_patient_name(&r.name), r.dob.clone()),
                new_idx,
            );
            self.by_id
                .insert((r.physician_id.clone(), r.patient_id.clone()), new_idx);
        }
        self.save()?;
        tracing::info!(
            event = "patient_deleted",
            physician_id = %physician_id,
            patient_id = %patient_id,
            name = %record.name,
            "patient record deleted"
        );
        Ok(true)
    }
}

/// Parity with `tauri-app/src-tauri/src/patient_name_tracker.rs::normalize_patient_name`.
/// Keep the two in sync — enforced by the parity-fixture test below.
pub fn normalize_patient_name(name: &str) -> String {
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

fn is_iso_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn manager() -> (PatientManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("patients.json");
        (PatientManager::load(path).unwrap(), tmp)
    }

    #[test]
    fn confirm_creates_record_on_first_call() {
        let (mut m, _tmp) = manager();
        let (rec, created) = m
            .confirm("phys-1", "Judie Joan Guest", "1945-04-08", "sess-a", None)
            .unwrap();
        assert!(created);
        assert_eq!(rec.name, "Judie Joan Guest");
        assert_eq!(rec.dob, "1945-04-08");
        assert_eq!(rec.session_ids, vec!["sess-a"]);
        assert!(rec.medplum_patient_id.is_none());
        assert!(!rec.patient_id.is_empty(), "UUID fallback assigned");
    }

    #[test]
    fn confirm_is_idempotent_on_same_key() {
        let (mut m, _tmp) = manager();
        let (r1, c1) = m
            .confirm("phys-1", "Judie Guest", "1945-04-08", "sess-a", None)
            .unwrap();
        let (r2, c2) = m
            .confirm("phys-1", "Judie Guest", "1945-04-08", "sess-b", None)
            .unwrap();
        assert!(c1);
        assert!(!c2);
        assert_eq!(r1.patient_id, r2.patient_id);
        assert_eq!(r2.session_ids, vec!["sess-a", "sess-b"]);
    }

    #[test]
    fn confirm_dedupes_repeat_session_ids() {
        let (mut m, _tmp) = manager();
        m.confirm("phys-1", "A", "1950-01-01", "sess-a", None).unwrap();
        m.confirm("phys-1", "A", "1950-01-01", "sess-a", None).unwrap();
        let rec = m.confirm("phys-1", "A", "1950-01-01", "sess-b", None).unwrap().0;
        assert_eq!(rec.session_ids, vec!["sess-a", "sess-b"]);
    }

    #[test]
    fn confirm_reconciles_medplum_id_later() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m
            .confirm("phys-1", "A", "1950-01-01", "sess-a", None)
            .unwrap();
        let uuid_patient_id = r1.patient_id.clone();
        let (r2, created) = m
            .confirm(
                "phys-1",
                "A",
                "1950-01-01",
                "sess-b",
                Some("mp-7".into()),
            )
            .unwrap();
        assert!(!created);
        assert_eq!(r2.patient_id, uuid_patient_id, "patient_id stable");
        assert_eq!(r2.medplum_patient_id.as_deref(), Some("mp-7"));
    }

    #[test]
    fn confirm_separates_different_dobs_with_same_name() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m.confirm("phys-1", "John Smith", "1970-01-01", "a", None).unwrap();
        let (r2, _) = m.confirm("phys-1", "John Smith", "1980-02-02", "b", None).unwrap();
        assert_ne!(r1.patient_id, r2.patient_id);
    }

    #[test]
    fn confirm_separates_physicians() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m.confirm("phys-1", "A", "1990-01-01", "a", None).unwrap();
        let (r2, _) = m.confirm("phys-2", "A", "1990-01-01", "a", None).unwrap();
        assert_ne!(r1.patient_id, r2.patient_id);
    }

    #[test]
    fn confirm_normalizes_name_variants() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m.confirm("phys-1", "  john SMITH ", "1970-01-01", "a", None).unwrap();
        let (r2, _) = m.confirm("phys-1", "John Smith", "1970-01-01", "b", None).unwrap();
        assert_eq!(r1.patient_id, r2.patient_id);
        assert_eq!(r2.session_ids, vec!["a", "b"]);
    }

    #[test]
    fn confirm_normalizes_surname_comma_given() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m.confirm("phys-1", "Guest, Judie Joan", "1945-04-08", "a", None).unwrap();
        let (r2, _) = m.confirm("phys-1", "Judie Joan Guest", "1945-04-08", "b", None).unwrap();
        assert_eq!(r1.patient_id, r2.patient_id);
        assert_eq!(r1.name, "Judie Joan Guest");
    }

    #[test]
    fn confirm_rejects_empty_physician_id() {
        let (mut m, _tmp) = manager();
        assert!(m.confirm("", "A", "1990-01-01", "a", None).is_err());
    }

    #[test]
    fn confirm_rejects_bad_dob_format() {
        let (mut m, _tmp) = manager();
        assert!(m.confirm("phys-1", "A", "not-a-date", "a", None).is_err());
        assert!(m.confirm("phys-1", "A", "04/08/1945", "a", None).is_err());
    }

    #[test]
    fn confirm_rejects_empty_name() {
        let (mut m, _tmp) = manager();
        assert!(m.confirm("phys-1", "   ", "1990-01-01", "a", None).is_err());
    }

    #[test]
    fn lookups_return_inserted_records() {
        let (mut m, _tmp) = manager();
        let (r, _) = m.confirm("phys-1", "Judie Guest", "1945-04-08", "a", Some("mp-9".into())).unwrap();
        assert_eq!(
            m.get_by_name_dob("phys-1", "Judie Guest", "1945-04-08")
                .map(|p| p.patient_id.clone()),
            Some(r.patient_id.clone())
        );
        assert_eq!(
            m.get_by_patient_id("phys-1", &r.patient_id)
                .map(|p| p.patient_id.clone()),
            Some(r.patient_id.clone())
        );
        assert!(m.get_by_name_dob("phys-1", "Other", "1945-04-08").is_none());
    }

    #[test]
    fn delete_removes_record_and_reindexes() {
        let (mut m, _tmp) = manager();
        let (r1, _) = m.confirm("phys-1", "Alice", "1990-01-01", "s1", None).unwrap();
        let (r2, _) = m.confirm("phys-1", "Bob", "1980-01-01", "s2", None).unwrap();
        assert!(m.delete("phys-1", &r1.patient_id).unwrap());
        // r1 gone, r2 still retrievable
        assert!(m.get_by_patient_id("phys-1", &r1.patient_id).is_none());
        assert_eq!(
            m.get_by_patient_id("phys-1", &r2.patient_id).map(|r| r.name),
            Some("Bob".to_string())
        );
        // Idempotent — second delete returns false
        assert!(!m.delete("phys-1", &r1.patient_id).unwrap());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let (mut m, _tmp) = manager();
        assert!(!m.delete("phys-1", "nope").unwrap());
    }

    #[test]
    fn list_for_physician_filters_by_owner() {
        let (mut m, _tmp) = manager();
        m.confirm("phys-1", "A", "1990-01-01", "a", None).unwrap();
        m.confirm("phys-1", "B", "1980-01-01", "b", None).unwrap();
        m.confirm("phys-2", "C", "1970-01-01", "c", None).unwrap();
        let p1 = m.list_for_physician("phys-1");
        assert_eq!(p1.len(), 2);
        assert!(p1.iter().all(|r| r.physician_id == "phys-1"));
    }

    #[test]
    fn persists_and_reloads() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("patients.json");
        {
            let mut m = PatientManager::load(path.clone()).unwrap();
            m.confirm("phys-1", "Judie Guest", "1945-04-08", "a", Some("mp-1".into()))
                .unwrap();
            m.confirm("phys-1", "Judie Guest", "1945-04-08", "b", None)
                .unwrap();
        }
        let m = PatientManager::load(path).unwrap();
        let rec = m.get_by_name_dob("phys-1", "Judie Guest", "1945-04-08").unwrap();
        assert_eq!(rec.session_ids, vec!["a", "b"]);
        assert_eq!(rec.medplum_patient_id.as_deref(), Some("mp-1"));
    }

    /// Verifies normalize_patient_name is byte-equivalent to the tauri-side
    /// implementation in patient_name_tracker.rs. Known inputs + expected
    /// outputs — if these diverge, idempotency of `confirm` breaks because
    /// tauri-client-reported "Judie Guest" won't match server-stored "Judie
    /// Joan Guest" and vice-versa.
    #[test]
    fn normalization_parity_with_tauri_client() {
        // Derived from tauri-app/src-tauri/src/patient_name_tracker.rs tests:
        //   test_patient_name_tracker_normalization  ("  john   SMITH  " → "John Smith")
        //   test_comma_format_normalization          ("Zamorano Sanchez, Claudia Marcela" →
        //                                             "Claudia Marcela Zamorano Sanchez")
        //   parse_vision_response handles "Surname, Given Middle" → "Given Middle Surname"
        let cases: &[(&str, &str)] = &[
            ("  john   SMITH  ", "John Smith"),
            (
                "Zamorano Sanchez, Claudia Marcela",
                "Claudia Marcela Zamorano Sanchez",
            ),
            ("JOHN SMITH", "John Smith"),
            ("judie Joan Guest", "Judie Joan Guest"),
            ("Guest, Judie", "Judie Guest"),
            ("", ""),
            ("   ", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(
                normalize_patient_name(input),
                *expected,
                "input={input:?}"
            );
        }
    }
}
