//! Tool-id ↔ sanitized-name mapping for LLM and MCP surfaces.
//!
//! LLM APIs (OpenAI function-calling, Anthropic tools, MCP) require
//! identifiers to match `[a-zA-Z0-9_-]`. ATD tool ids use `:` and `.`
//! for namespace/domain/action structure. This module translates in
//! both directions.
//!
//! Note that sanitization is lossy — `a:b` and `a.b` both map to
//! `a_b`. Reverse lookup therefore requires the caller to provide
//! the set of known original ids.

/// Map an ATD tool id to an LLM-/MCP-safe name.
///
/// Rules:
/// - `:` → `_`
/// - `.` → `_`
/// - any other character outside `[a-zA-Z0-9_-]` → `_`
///
/// Examples:
/// - `ref:fs.read` → `ref_fs_read`
/// - `xiaomi:light.toggle` → `xiaomi_light_toggle`
pub fn sanitize_tool_name(tool_id: &str) -> String {
    tool_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Map a sanitized name back to the original tool id by searching
/// the provided known-id set. Returns `None` if no match.
///
/// Ambiguity: if multiple original ids sanitize to the same form,
/// returns the first match from the iteration order of `known`.
/// Callers that care about stability should sort `known`.
pub fn desanitize_tool_name<'a, I>(sanitized: &str, known: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    for id in known {
        if sanitize_tool_name(id) == sanitized {
            return Some(id);
        }
    }
    None
}

/// Check whether sanitization would cause a collision within the given
/// set of ids. Useful for detecting adapter-shape problems before they
/// surface as confusing LLM behavior.
pub fn detect_collisions<'a, I>(ids: I) -> Vec<(String, Vec<&'a str>)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut groups: std::collections::HashMap<String, Vec<&'a str>> =
        std::collections::HashMap::new();
    for id in ids {
        groups.entry(sanitize_tool_name(id)).or_default().push(id);
    }
    groups.into_iter().filter(|(_, v)| v.len() > 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_passes_through() {
        assert_eq!(sanitize_tool_name("echo_say"), "echo_say");
        assert_eq!(sanitize_tool_name("tool-name"), "tool-name");
    }

    #[test]
    fn colon_and_dot_become_underscore() {
        assert_eq!(sanitize_tool_name("ref:fs.read"), "ref_fs_read");
        assert_eq!(
            sanitize_tool_name("xiaomi:light.toggle"),
            "xiaomi_light_toggle"
        );
    }

    #[test]
    fn multiple_special_chars() {
        assert_eq!(sanitize_tool_name("a/b c+d"), "a_b_c_d");
    }

    #[test]
    fn desanitize_round_trips_via_known_list() {
        let known = &["ref:fs.read", "ref:shell.exec", "ref:echo.say"];
        let hit = desanitize_tool_name("ref_shell_exec", known.iter().copied());
        assert_eq!(hit, Some("ref:shell.exec"));
    }

    #[test]
    fn desanitize_returns_none_for_unknown() {
        let known = &["ref:fs.read"];
        assert!(desanitize_tool_name("something_else", known.iter().copied()).is_none());
    }

    #[test]
    fn collisions_are_detected() {
        let ids = &["a:b", "a.b", "a_b", "c:d"];
        let collisions = detect_collisions(ids.iter().copied());
        assert_eq!(collisions.len(), 1);
        let (sanitized, members) = &collisions[0];
        assert_eq!(sanitized, "a_b");
        assert_eq!(members.len(), 3);
    }
}
