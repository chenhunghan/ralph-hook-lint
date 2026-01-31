/// Extract `file_path` from JSON like `{"tool_input":{"file_path":"/some/path"}}`
pub fn extract_file_path(json: &str) -> Option<String> {
    let marker = r#""file_path":"#;
    let start = json.find(marker)? + marker.len();
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
}
