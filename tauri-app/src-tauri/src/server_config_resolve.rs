use crate::config::Settings;
use crate::server_config::OperationalDefaults;

/// Resolves the effective value for a user-tunable field according to the
/// Phase 3 precedence model:
///   compiled default  <  server value  <  local config.json (only if user-edited)
///
/// Returns:
/// - `local.clone()` if `user_edited.contains(field_name)` — user's tune always wins.
/// - `server.clone()` if `server` is `Some` — clinic-wide default when server is reachable.
/// - `local.clone()` otherwise — which equals the compiled default for non-edited fields.
pub fn resolve<T: Clone>(
    server: Option<&T>,
    local: &T,
    field_name: &str,
    user_edited: &[String],
) -> T {
    if user_edited.iter().any(|f| f == field_name) {
        local.clone()
    } else if let Some(s) = server {
        s.clone()
    } else {
        local.clone()
    }
}

/// Resolves all 10 Category B fields into a fresh `OperationalDefaults`.
///
/// Each field is routed through [`resolve`] so the precedence rule
/// (compiled default < server < local-when-edited) holds per field. `version`
/// is copied straight from the server (or 0 when server is absent) since it's
/// metadata, not user-tunable.
///
/// Callers typically take `server` as `Option<&OperationalDefaults>` pulled
/// from `SharedServerConfig` so that a missing server-config section still
/// yields a sane result (local values, which equal compiled defaults for
/// non-edited fields).
pub fn resolve_operational(
    settings: &Settings,
    server: Option<&OperationalDefaults>,
) -> OperationalDefaults {
    let edited = &settings.user_edited_fields;
    OperationalDefaults {
        version: server.map(|s| s.version).unwrap_or(0),
        sleep_start_hour: resolve(
            server.map(|s| &s.sleep_start_hour),
            &settings.sleep_start_hour,
            "sleep_start_hour",
            edited,
        ),
        sleep_end_hour: resolve(
            server.map(|s| &s.sleep_end_hour),
            &settings.sleep_end_hour,
            "sleep_end_hour",
            edited,
        ),
        thermal_hot_pixel_threshold_c: resolve(
            server.map(|s| &s.thermal_hot_pixel_threshold_c),
            &settings.thermal_hot_pixel_threshold_c,
            "thermal_hot_pixel_threshold_c",
            edited,
        ),
        co2_baseline_ppm: resolve(
            server.map(|s| &s.co2_baseline_ppm),
            &settings.co2_baseline_ppm,
            "co2_baseline_ppm",
            edited,
        ),
        encounter_check_interval_secs: resolve(
            server.map(|s| &s.encounter_check_interval_secs),
            &settings.encounter_check_interval_secs,
            "encounter_check_interval_secs",
            edited,
        ),
        encounter_silence_trigger_secs: resolve(
            server.map(|s| &s.encounter_silence_trigger_secs),
            &settings.encounter_silence_trigger_secs,
            "encounter_silence_trigger_secs",
            edited,
        ),
        soap_model: resolve(
            server.map(|s| &s.soap_model),
            &settings.soap_model,
            "soap_model",
            edited,
        ),
        soap_model_fast: resolve(
            server.map(|s| &s.soap_model_fast),
            &settings.soap_model_fast,
            "soap_model_fast",
            edited,
        ),
        fast_model: resolve(
            server.map(|s| &s.fast_model),
            &settings.fast_model,
            "fast_model",
            edited,
        ),
        encounter_detection_model: resolve(
            server.map(|s| &s.encounter_detection_model),
            &settings.encounter_detection_model,
            "encounter_detection_model",
            edited,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_returns_local_when_user_edited() {
        let edited = vec!["foo".to_string()];
        assert_eq!(resolve(Some(&100), &50, "foo", &edited), 50);
    }

    #[test]
    fn test_resolve_returns_server_when_not_edited_and_server_present() {
        let edited: Vec<String> = vec![];
        assert_eq!(resolve(Some(&100), &50, "foo", &edited), 100);
    }

    #[test]
    fn test_resolve_returns_local_when_not_edited_and_server_absent() {
        let edited: Vec<String> = vec![];
        assert_eq!(resolve::<i32>(None, &50, "foo", &edited), 50);
    }

    #[test]
    fn test_resolve_field_isolation() {
        let edited = vec!["foo".to_string()];
        assert_eq!(resolve(Some(&100), &50, "foo", &edited), 50);
        assert_eq!(resolve(Some(&200), &75, "bar", &edited), 200);
    }

    #[test]
    fn test_resolve_with_strings() {
        let edited: Vec<String> = vec![];
        let server = "server-model".to_string();
        let local = "local-model".to_string();
        assert_eq!(resolve(Some(&server), &local, "alias", &edited), "server-model");
    }

    #[test]
    fn test_resolve_with_floats() {
        let edited: Vec<String> = vec![];
        let server: f32 = 0.85;
        let local: f32 = 0.5;
        assert_eq!(resolve(Some(&server), &local, "threshold", &edited), 0.85);
    }

    // ── resolve_operational ──────────────────────────────────────────

    fn make_server_defaults() -> OperationalDefaults {
        OperationalDefaults {
            version: 7,
            sleep_start_hour: 21,
            sleep_end_hour: 7,
            thermal_hot_pixel_threshold_c: 29.5,
            co2_baseline_ppm: 450.0,
            encounter_check_interval_secs: 90,
            encounter_silence_trigger_secs: 30,
            soap_model: "server-soap".to_string(),
            soap_model_fast: "server-soap-fast".to_string(),
            fast_model: "server-fast".to_string(),
            encounter_detection_model: "server-detect".to_string(),
        }
    }

    fn make_local_settings() -> Settings {
        let mut s = Settings::default();
        // Give each field a distinct "tuned" value so we can tell server vs local apart.
        s.sleep_start_hour = 20;
        s.sleep_end_hour = 8;
        s.thermal_hot_pixel_threshold_c = 27.0;
        s.co2_baseline_ppm = 410.0;
        s.encounter_check_interval_secs = 60;
        s.encounter_silence_trigger_secs = 20;
        s.soap_model = "local-soap".to_string();
        s.soap_model_fast = "local-soap-fast".to_string();
        s.fast_model = "local-fast".to_string();
        s.encounter_detection_model = "local-detect".to_string();
        s
    }

    #[test]
    fn test_resolve_operational_all_server_when_no_user_edits() {
        // No local edits → server values win for every Cat B field.
        let settings = make_local_settings();
        let server = make_server_defaults();
        let resolved = resolve_operational(&settings, Some(&server));

        assert_eq!(resolved.version, 7);
        assert_eq!(resolved.sleep_start_hour, 21);
        assert_eq!(resolved.sleep_end_hour, 7);
        assert!((resolved.thermal_hot_pixel_threshold_c - 29.5).abs() < f32::EPSILON);
        assert!((resolved.co2_baseline_ppm - 450.0).abs() < f32::EPSILON);
        assert_eq!(resolved.encounter_check_interval_secs, 90);
        assert_eq!(resolved.encounter_silence_trigger_secs, 30);
        assert_eq!(resolved.soap_model, "server-soap");
        assert_eq!(resolved.soap_model_fast, "server-soap-fast");
        assert_eq!(resolved.fast_model, "server-fast");
        assert_eq!(resolved.encounter_detection_model, "server-detect");
    }

    #[test]
    fn test_resolve_operational_user_edited_fields_win() {
        // User tuned 3 fields locally — those must stick even when server sends new values.
        let mut settings = make_local_settings();
        settings.user_edited_fields = vec![
            "sleep_start_hour".to_string(),
            "soap_model".to_string(),
            "co2_baseline_ppm".to_string(),
        ];
        let server = make_server_defaults();
        let resolved = resolve_operational(&settings, Some(&server));

        // User-edited — local values
        assert_eq!(resolved.sleep_start_hour, 20, "user-edited local value should win");
        assert_eq!(resolved.soap_model, "local-soap", "user-edited local value should win");
        assert!(
            (resolved.co2_baseline_ppm - 410.0).abs() < f32::EPSILON,
            "user-edited float should take local value"
        );

        // Not user-edited — server values
        assert_eq!(resolved.sleep_end_hour, 7);
        assert_eq!(resolved.encounter_check_interval_secs, 90);
        assert_eq!(resolved.soap_model_fast, "server-soap-fast");
        assert_eq!(resolved.fast_model, "server-fast");
    }

    #[test]
    fn test_resolve_operational_falls_back_to_local_when_server_absent() {
        // Server unreachable — every field falls back to the local value.
        let settings = make_local_settings();
        let resolved = resolve_operational(&settings, None);

        assert_eq!(resolved.version, 0, "no server → version 0");
        assert_eq!(resolved.sleep_start_hour, 20);
        assert_eq!(resolved.sleep_end_hour, 8);
        assert!((resolved.thermal_hot_pixel_threshold_c - 27.0).abs() < f32::EPSILON);
        assert!((resolved.co2_baseline_ppm - 410.0).abs() < f32::EPSILON);
        assert_eq!(resolved.encounter_check_interval_secs, 60);
        assert_eq!(resolved.encounter_silence_trigger_secs, 20);
        assert_eq!(resolved.soap_model, "local-soap");
        assert_eq!(resolved.soap_model_fast, "local-soap-fast");
        assert_eq!(resolved.fast_model, "local-fast");
        assert_eq!(resolved.encounter_detection_model, "local-detect");
    }

    #[test]
    fn test_resolve_operational_default_settings_with_no_server_equals_compiled_defaults() {
        // Sanity: fresh Settings + no server should produce the compiled OperationalDefaults.
        // Guards against silent compiled-default drift between the two modules.
        let settings = Settings::default();
        let resolved = resolve_operational(&settings, None);
        let compiled = OperationalDefaults::default();
        // version comes from server (None → 0); compiled default is also 0
        assert_eq!(resolved, compiled);
    }

    #[test]
    fn test_resolve_operational_all_10_cat_b_fields_covered() {
        // If someone adds a Cat B field to OperationalDefaults but forgets to
        // wire it in resolve_operational, this catches it: every server value
        // must round-trip distinctly through the resolver.
        let settings = Settings::default();
        let server = make_server_defaults();
        let resolved = resolve_operational(&settings, Some(&server));
        // Every field on the server struct must appear on the resolved struct
        // with the same value (since no user edits).
        assert_eq!(resolved, server);
    }

    // ── Phase 3 T8 end-to-end precedence tests ───────────────────────
    //
    // These tests pin the exact integration contract described in the T8
    // plan (items 4-6): user-edited fields always win over the server, and
    // a mixed scenario resolves each of the 10 Cat B fields independently.

    #[test]
    fn test_resolve_operational_user_edited_fields_win_regardless_of_server() {
        // Exact scenario from the T8 plan: user tuned sleep_start_hour to 21
        // locally. Server pushes a new clinic-wide default of 23. User's
        // local edit must win, unconditionally.
        let mut settings = Settings::default();
        settings.sleep_start_hour = 21;
        settings.user_edited_fields = vec!["sleep_start_hour".to_string()];

        let mut server = OperationalDefaults::default();
        server.sleep_start_hour = 23;

        let resolved = resolve_operational(&settings, Some(&server));
        assert_eq!(
            resolved.sleep_start_hour, 21,
            "user-edited local value must win even when server pushes a different value"
        );
    }

    #[test]
    fn test_resolve_operational_mixed() {
        // Phase 3 T8 item 6: verify the resolver handles every one of the 10
        // Cat B fields independently in a mixed scenario. We partition the
        // fields into two halves — 5 user-edited (local wins) and 5 unedited
        // (server wins) — and assert each resolves correctly.
        //
        // If a future refactor accidentally couples fields (e.g. by sharing a
        // resolve closure and feeding the wrong field_name into one of them),
        // the asymmetric edit set here catches it: the "edited" half would
        // fall through to the server value or vice versa.
        let mut settings = make_local_settings();
        settings.user_edited_fields = vec![
            "sleep_start_hour".to_string(),
            "thermal_hot_pixel_threshold_c".to_string(),
            "encounter_check_interval_secs".to_string(),
            "soap_model".to_string(),
            "fast_model".to_string(),
        ];
        let server = make_server_defaults();
        let resolved = resolve_operational(&settings, Some(&server));

        // ── Edited: local values must win ─────────────────────────
        assert_eq!(resolved.sleep_start_hour, 20);
        assert!(
            (resolved.thermal_hot_pixel_threshold_c - 27.0).abs() < f32::EPSILON,
            "edited float must take local value"
        );
        assert_eq!(resolved.encounter_check_interval_secs, 60);
        assert_eq!(resolved.soap_model, "local-soap");
        assert_eq!(resolved.fast_model, "local-fast");

        // ── Unedited: server values must win ──────────────────────
        assert_eq!(resolved.sleep_end_hour, 7);
        assert!(
            (resolved.co2_baseline_ppm - 450.0).abs() < f32::EPSILON,
            "unedited float must take server value"
        );
        assert_eq!(resolved.encounter_silence_trigger_secs, 30);
        assert_eq!(resolved.soap_model_fast, "server-soap-fast");
        assert_eq!(resolved.encounter_detection_model, "server-detect");

        // Version is metadata, always copied from server.
        assert_eq!(resolved.version, 7);
    }
}
