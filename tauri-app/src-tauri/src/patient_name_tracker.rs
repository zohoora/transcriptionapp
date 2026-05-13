//! Patient-name normalization + balanced-JSON extraction helpers shared
//! between `llm_client::sanitize_extracted_patient_name` and
//! `medication_extraction::parse_response`.

/// Normalize a patient name: handle "Last, First" → "First Last" format,
/// trim whitespace, collapse multiple spaces, title-case. Used to bring the
/// SOAP-extracted name to a canonical shape before it lands in
/// `ArchiveMetadata.patient_name`.
pub fn normalize_patient_name(name: &str) -> String {
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

/// Find the first balanced bracket-delimited region of `s`, respecting string
/// escaping. Used for both `{...}` and `[...]` JSON extraction across vision
/// response parsers.
///
/// Handles markdown-wrapped JSON, leading garbage, and multi-block responses.
pub fn extract_first_balanced(s: &str, open: u8, close: u8) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == open)?;

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
        if b == b'"' {
            in_string = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                let end = start + idx + 1;
                return Some(&s[start..end]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_comma_format() {
        assert_eq!(
            normalize_patient_name("Zamorano Sanchez, Claudia Marcela"),
            "Claudia Marcela Zamorano Sanchez"
        );
    }

    #[test]
    fn normalize_lowercase_and_whitespace() {
        assert_eq!(normalize_patient_name("  john   SMITH  "), "John Smith");
    }

    #[test]
    fn normalize_empty() {
        assert_eq!(normalize_patient_name(""), "");
    }

    #[test]
    fn extract_balanced_braces() {
        let s = r#"{"a": 1, "b": {"c": 2}}"#;
        assert_eq!(extract_first_balanced(s, b'{', b'}'), Some(s));
    }

    #[test]
    fn extract_balanced_with_leading_garbage() {
        let s = r#"here is the json: {"name": "Jane Doe"}"#;
        assert_eq!(
            extract_first_balanced(s, b'{', b'}'),
            Some(r#"{"name": "Jane Doe"}"#)
        );
    }

    #[test]
    fn extract_balanced_markdown_fence_first_object() {
        // Two JSON blocks concatenated between markdown fences — the
        // historic Apr 20 Room 6 vision response shape. We want the FIRST
        // balanced object back.
        let s = r#"```json
{"name": "Judie Joan Guest", "dob": "1945-04-08"}
```

```json
{"name": "judie Joan Guest", "dob": "1945-04-08"}
```"#;
        let got = extract_first_balanced(s, b'{', b'}').expect("found first object");
        let parsed: serde_json::Value = serde_json::from_str(got).expect("parses");
        assert_eq!(parsed["name"], "Judie Joan Guest");
    }

    #[test]
    fn extract_balanced_string_with_braces_escaped() {
        // Braces inside a JSON string value should not prematurely close.
        let s = r#"{"name": "Weird { Name }", "dob": "1990-01-01"}"#;
        assert_eq!(extract_first_balanced(s, b'{', b'}'), Some(s));
    }

    #[test]
    fn extract_balanced_returns_none_on_unbalanced() {
        assert_eq!(extract_first_balanced("{", b'{', b'}'), None);
        assert_eq!(extract_first_balanced("{ \"a\": 1 ", b'{', b'}'), None);
        assert_eq!(extract_first_balanced("no braces here", b'{', b'}'), None);
    }

    #[test]
    fn extract_balanced_brackets() {
        let s = r#"[1, [2, 3], 4]"#;
        assert_eq!(extract_first_balanced(s, b'[', b']'), Some(s));
    }
}
