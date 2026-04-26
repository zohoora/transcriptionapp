//! Prompt-variant resolution for experiment CLIs.

use std::path::PathBuf;
use anyhow::{anyhow, Result};

/// Where the prompt body comes from. Built-in names are interpreted by each
/// CLI's `Runner` impl (e.g., `soap_experiment_cli` understands "baseline",
/// "v0_10_61"; `billing_experiment_cli` understands "visit_dx", "strict").
/// File paths and inline strings are passed through verbatim.
#[derive(Debug, Clone)]
pub enum VariantSource {
    /// Named built-in variant (e.g., "baseline", "visit_dx"). Each CLI's
    /// `Runner` resolves the name to an actual prompt string.
    Builtin(String),
    /// Path to a file containing the prompt body.
    File(PathBuf),
    /// Inline prompt body (typically when called programmatically).
    Inline(String),
}

#[derive(Debug, Clone)]
pub struct Variant {
    /// Display label for reporting (e.g., "baseline", "v0_10_61", "my-test.txt").
    pub label: String,
    pub source: VariantSource,
}

impl Variant {
    pub fn builtin(name: &str) -> Self {
        Self {
            label: name.to_string(),
            source: VariantSource::Builtin(name.to_string()),
        }
    }

    pub fn file(path: PathBuf) -> Self {
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        Self { label, source: VariantSource::File(path) }
    }

    pub fn inline(label: &str, body: &str) -> Self {
        Self {
            label: label.to_string(),
            source: VariantSource::Inline(body.to_string()),
        }
    }

    /// Materialize the prompt body. For [`VariantSource::Builtin`], `resolver`
    /// is invoked with the built-in name; the CLI provides the resolver.
    pub fn body<F>(&self, resolver: F) -> Result<String>
    where
        F: FnOnce(&str) -> Result<String>,
    {
        match &self.source {
            VariantSource::Builtin(name) => resolver(name),
            VariantSource::File(path) => std::fs::read_to_string(path)
                .map_err(|e| anyhow!("Read variant file {}: {e}", path.display())),
            VariantSource::Inline(body) => Ok(body.clone()),
        }
    }
}

/// Parse a single `--variant <ARG>` value. The arg is treated as a file path
/// when it ends in `.txt`, `.md`, `.prompt`, or contains a path separator AND
/// the file exists. Otherwise it's treated as a built-in name.
pub fn parse_variant_arg(arg: &str) -> Variant {
    let p = PathBuf::from(arg);
    let looks_like_path = arg.contains('/')
        || arg.ends_with(".txt")
        || arg.ends_with(".md")
        || arg.ends_with(".prompt");
    if looks_like_path && p.exists() {
        Variant::file(p)
    } else {
        Variant::builtin(arg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_variant() {
        let v = parse_variant_arg("baseline");
        assert_eq!(v.label, "baseline");
        assert!(matches!(v.source, VariantSource::Builtin(_)));
    }

    #[test]
    fn test_file_variant_resolves_label() {
        let p = PathBuf::from("/tmp/some_prompt.txt");
        let v = Variant::file(p);
        assert_eq!(v.label, "some_prompt");
    }

    #[test]
    fn test_inline_variant_carries_body() {
        let v = Variant::inline("test", "system prompt body");
        let body = v.body(|_| Err(anyhow!("should not call resolver"))).unwrap();
        assert_eq!(body, "system prompt body");
    }

    #[test]
    fn test_builtin_dispatches_to_resolver() {
        let v = Variant::builtin("strict");
        let body = v.body(|name| {
            assert_eq!(name, "strict");
            Ok("strict prompt".to_string())
        }).unwrap();
        assert_eq!(body, "strict prompt");
    }
}
