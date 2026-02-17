/// Extract a JSON string field value by key name from raw JSON text.
/// Searches for `"field_name":` and parses the quoted string value.
fn extract_string_field(json: &str, field_name: &str) -> Option<String> {
    let marker = format!(r#""{field_name}":"#);
    let start = json.find(&marker)? + marker.len();
    let rest = &json[start..];

    // Skip whitespace
    let rest = rest.trim_start();

    // Expect a quote
    if !rest.starts_with('"') {
        return None;
    }

    let rest = &rest[1..];
    let mut result = String::new();
    let mut chars = rest.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(result),
            '\\' => {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        '/' => result.push('/'),
                        _ => {
                            result.push('\\');
                            result.push(escaped);
                        }
                    }
                }
            }
            _ => result.push(c),
        }
    }
    None
}

/// Extract `file_path` from JSON like `{"tool_input":{"file_path":"/some/path"}}`
pub fn extract_file_path(json: &str) -> Option<String> {
    extract_string_field(json, "file_path")
}

/// Extract `session_id` from JSON like `{"session_id":"abc123"}`
pub fn extract_session_id(json: &str) -> Option<String> {
    extract_string_field(json, "session_id")
}

/// Extract `reason` from a block JSON like `{"decision":"block","reason":"..."}`
pub fn extract_reason_field(json: &str) -> Option<String> {
    extract_string_field(json, "reason")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_file_path() {
        let json = r#"{"tool_input":{"file_path":"/some/path.ts"}}"#;
        assert_eq!(extract_file_path(json), Some("/some/path.ts".to_string()));
    }

    #[test]
    fn file_path_with_whitespace_after_colon() {
        let json = r#"{"file_path": "/path/to/file.js"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/to/file.js".to_string())
        );
    }

    #[test]
    fn file_path_with_spaces_in_path() {
        let json = r#"{"file_path":"/path/with spaces/file.ts"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/with spaces/file.ts".to_string())
        );
    }

    #[test]
    fn escaped_backslash_in_path() {
        let json = r#"{"file_path":"C:\\Users\\test\\file.ts"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("C:\\Users\\test\\file.ts".to_string())
        );
    }

    #[test]
    fn escaped_quote_in_path() {
        let json = r#"{"file_path":"/path/with\"quote/file.ts"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/with\"quote/file.ts".to_string())
        );
    }

    #[test]
    fn escaped_newline_in_path() {
        let json = r#"{"file_path":"/path/with\nnewline"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/with\nnewline".to_string())
        );
    }

    #[test]
    fn escaped_tab_in_path() {
        let json = r#"{"file_path":"/path/with\ttab"}"#;
        assert_eq!(extract_file_path(json), Some("/path/with\ttab".to_string()));
    }

    #[test]
    fn escaped_forward_slash() {
        let json = r#"{"file_path":"\/path\/to\/file.ts"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/to/file.ts".to_string())
        );
    }

    #[test]
    fn no_file_path_key() {
        let json = r#"{"other_key":"value"}"#;
        assert_eq!(extract_file_path(json), None);
    }

    #[test]
    fn empty_file_path() {
        let json = r#"{"file_path":""}"#;
        assert_eq!(extract_file_path(json), Some(String::new()));
    }

    #[test]
    fn missing_closing_quote() {
        let json = r#"{"file_path":"/path/incomplete"#;
        assert_eq!(extract_file_path(json), None);
    }

    #[test]
    fn non_string_value() {
        let json = r#"{"file_path":123}"#;
        assert_eq!(extract_file_path(json), None);
    }

    #[test]
    fn null_value() {
        let json = r#"{"file_path":null}"#;
        assert_eq!(extract_file_path(json), None);
    }

    #[test]
    fn deeply_nested() {
        let json = r#"{"outer":{"inner":{"tool_input":{"file_path":"/nested/path.tsx"}}}}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/nested/path.tsx".to_string())
        );
    }

    #[test]
    fn real_world_hook_input() {
        let json = r#"{"tool_name":"Write","tool_input":{"file_path":"/Users/test/project/src/index.ts","content":"console.log('hello');"}}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/Users/test/project/src/index.ts".to_string())
        );
    }

    #[test]
    fn unknown_escape_sequence() {
        let json = r#"{"file_path":"/path/with\xunknown"}"#;
        assert_eq!(
            extract_file_path(json),
            Some("/path/with\\xunknown".to_string())
        );
    }

    // Tests for extract_session_id

    #[test]
    fn basic_session_id() {
        let json = r#"{"session_id":"abc-123-def"}"#;
        assert_eq!(extract_session_id(json), Some("abc-123-def".to_string()));
    }

    #[test]
    fn session_id_in_hook_input() {
        let json =
            r#"{"session_id":"sess42","tool_name":"Edit","tool_input":{"file_path":"/tmp/f.rs"}}"#;
        assert_eq!(extract_session_id(json), Some("sess42".to_string()));
    }

    #[test]
    fn session_id_missing() {
        let json = r#"{"tool_name":"Edit"}"#;
        assert_eq!(extract_session_id(json), None);
    }

    #[test]
    fn session_id_empty() {
        let json = r#"{"session_id":""}"#;
        assert_eq!(extract_session_id(json), Some(String::new()));
    }

    #[test]
    fn session_id_with_special_chars() {
        let json = r#"{"session_id":"a\/b\\c\"d"}"#;
        assert_eq!(extract_session_id(json), Some("a/b\\c\"d".to_string()));
    }
}
