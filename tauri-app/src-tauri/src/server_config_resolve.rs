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
}
