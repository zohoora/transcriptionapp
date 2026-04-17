//! Settings commands

use super::CommandError;
use crate::commands::physicians::SharedServerConfig;
use crate::config::{cat_b_field_eq, Config, Settings, CAT_B_FIELD_NAMES};
use crate::server_config::OperationalDefaults;
use tauri::State;

/// Get current settings
#[tauri::command]
pub fn get_settings() -> Result<Settings, CommandError> {
    let config = Config::load_or_default();
    Ok(config.to_settings())
}

/// Update settings.
///
/// Phase 3 diff-on-save: before writing, compare each Cat B field against the
/// currently-stored value. Any field whose value changed is appended to
/// `user_edited_fields` (dedup'd). This lets server-pushed
/// `OperationalDefaults` skip user-tuned values on future refreshes.
#[tauri::command]
pub fn set_settings(mut settings: Settings) -> Result<Settings, CommandError> {
    // Validate settings before saving
    let validation_errors = settings.validate();
    if !validation_errors.is_empty() {
        let error_messages: Vec<String> =
            validation_errors.iter().map(|e| e.to_string()).collect();
        return Err(CommandError::Validation(format!(
            "Invalid settings: {}",
            error_messages.join("; ")
        )));
    }

    // Diff-on-save: find Cat B fields whose value changed vs. currently-stored
    // settings and record them as user-edited. Uses the existing settings on
    // disk as the reference.
    let existing = Config::load_or_default();
    merge_user_edited_fields(&mut settings, &existing.settings);

    let mut config = existing;
    config.update_from_settings(&settings);
    config
        .save()
        .map_err(|e| CommandError::Config(e.to_string()))?;
    Ok(config.to_settings())
}

/// Clear a field name from `user_edited_fields`.
///
/// After calling this the server-configurable resolver treats the field as
/// "reset to server/compiled default" — subsequent server pushes are free to
/// overwrite the local value. No-op if the field isn't tracked.
#[tauri::command]
pub fn clear_user_edited_field(field_name: String) -> Result<Settings, CommandError> {
    let mut config = Config::load_or_default();
    let before = config.settings.user_edited_fields.len();
    config
        .settings
        .user_edited_fields
        .retain(|f| f != &field_name);
    if config.settings.user_edited_fields.len() != before {
        config
            .save()
            .map_err(|e| CommandError::Config(e.to_string()))?;
    }
    Ok(config.to_settings())
}

/// Return the current server-supplied `OperationalDefaults`.
///
/// Reads from the Tauri-managed `SharedServerConfig` — which itself falls back
/// through cache → compiled defaults, so this command always succeeds. Used by
/// the Settings UI to show "Clinic default: …" hints next to Cat B inputs and
/// to power the "Reset to clinic default" link.
///
/// Phase 3 note: only the four Cat B fields that have visible inputs in the
/// settings drawer currently surface this value to the user. The remaining six
/// Cat B fields (`sleep_*`, `encounter_*`, `soap_model_fast`,
/// `encounter_detection_model`) are still server-controllable via
/// `PUT /config/defaults` on the profile service — they just lack a UI surface
/// until a future phase.
#[tauri::command]
pub async fn get_operational_defaults(
    server_config: State<'_, SharedServerConfig>,
) -> Result<OperationalDefaults, CommandError> {
    Ok(server_config.read().await.defaults.clone())
}

/// Merges changed-field tracking from `previous` into `new`:
/// - For each Cat B field: if `new.X != previous.X`, append field name to `new.user_edited_fields`.
/// - Union with `previous.user_edited_fields` so an oblivious frontend that omits
///   the field in its round-trip doesn't erase tracking state.
///
/// Dedup'd so repeated edits don't create duplicates.
///
/// # Known race
///
/// If `clear_user_edited_field("X")` runs and then a `set_settings` call arrives
/// whose payload was built from a stale snapshot (pre-clear), this function will:
/// 1. Union with the now-empty previous list → no effect.
/// 2. But if the stale payload's Cat B value `X` differs from the current on-disk
///    value (because the frontend's pre-clear snapshot had the pre-clear value),
///    the diff-on-save branch re-adds field name "X" to user_edited_fields.
///
/// T7 (admin UI) is responsible for re-fetching settings after a `clear_user_edited_field`
/// call before allowing another `set_settings` to be issued. This function treats
/// every `set_settings` payload as authoritative at its construction time.
///
/// Closing this race properly would require a version counter on
/// `user_edited_fields` — a larger design change deferred past Phase 3.
fn merge_user_edited_fields(new: &mut Settings, previous: &Settings) {
    // Start from the existing list so an oblivious frontend doesn't erase it.
    for field in &previous.user_edited_fields {
        if !new.user_edited_fields.iter().any(|f| f == field) {
            new.user_edited_fields.push(field.clone());
        }
    }
    // Diff Cat B fields and append any that changed.
    for &field in CAT_B_FIELD_NAMES {
        if !cat_b_field_eq(new, previous, field)
            && !new.user_edited_fields.iter().any(|f| f == field)
        {
            new.user_edited_fields.push(field.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_settings_adds_changed_field_to_user_edited() {
        let previous = Settings::default();
        let mut new = previous.clone();
        new.thermal_hot_pixel_threshold_c = 29.0;

        merge_user_edited_fields(&mut new, &previous);

        assert!(new
            .user_edited_fields
            .iter()
            .any(|f| f == "thermal_hot_pixel_threshold_c"));
    }

    #[test]
    fn test_set_settings_dedup() {
        let previous = Settings::default();
        let mut new = previous.clone();
        new.thermal_hot_pixel_threshold_c = 29.0;

        // First pass seeds the field
        merge_user_edited_fields(&mut new, &previous);
        assert_eq!(
            new.user_edited_fields
                .iter()
                .filter(|f| f.as_str() == "thermal_hot_pixel_threshold_c")
                .count(),
            1
        );

        // Second pass on the same inputs must NOT add a duplicate
        merge_user_edited_fields(&mut new, &previous);
        assert_eq!(
            new.user_edited_fields
                .iter()
                .filter(|f| f.as_str() == "thermal_hot_pixel_threshold_c")
                .count(),
            1,
            "merge should be idempotent — no duplicate entries"
        );
    }

    #[test]
    fn test_set_settings_preserves_existing_when_frontend_omits_field() {
        // Simulates an older frontend that doesn't know about
        // user_edited_fields: incoming Settings has an empty Vec, but the
        // previously-saved Settings has tracking state we must keep.
        let mut previous = Settings::default();
        previous
            .user_edited_fields
            .push("sleep_start_hour".to_string());

        let mut new = previous.clone();
        new.user_edited_fields.clear();
        // No Cat B value changed in this payload — the only reason the list
        // should end up non-empty is preservation from `previous`.

        merge_user_edited_fields(&mut new, &previous);

        assert_eq!(new.user_edited_fields, vec!["sleep_start_hour"]);
    }

    #[test]
    fn test_clear_user_edited_field_removes_entry() {
        let mut settings = Settings::default();
        settings
            .user_edited_fields
            .push("thermal_hot_pixel_threshold_c".to_string());
        settings
            .user_edited_fields
            .push("sleep_start_hour".to_string());

        // Emulate the command body (the tauri::command wrapper isn't callable
        // as a plain function, but the logic we want to cover is the
        // retain + save decision).
        let before = settings.user_edited_fields.len();
        settings
            .user_edited_fields
            .retain(|f| f != "thermal_hot_pixel_threshold_c");
        let changed = settings.user_edited_fields.len() != before;

        assert!(changed, "should have removed the entry");
        assert!(!settings
            .user_edited_fields
            .iter()
            .any(|f| f == "thermal_hot_pixel_threshold_c"));
        assert!(settings
            .user_edited_fields
            .iter()
            .any(|f| f == "sleep_start_hour"));
    }

    #[test]
    fn test_clear_user_edited_field_noop_on_missing() {
        let mut settings = Settings::default();
        settings
            .user_edited_fields
            .push("sleep_start_hour".to_string());

        let before = settings.user_edited_fields.len();
        settings.user_edited_fields.retain(|f| f != "does_not_exist");
        let changed = settings.user_edited_fields.len() != before;

        assert!(!changed, "missing field should be a silent no-op");
        assert_eq!(settings.user_edited_fields, vec!["sleep_start_hour"]);
    }
}
