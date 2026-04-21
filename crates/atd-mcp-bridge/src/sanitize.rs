//! Tool-name sanitization for MCP compatibility.
//!
//! MCP allows `[a-zA-Z0-9_-]{1,128}`. ATD ids use `:` and `.` — we replace
//! them with `_` on outbound and reverse on inbound. Ported logic from
//! `/home/nan/proj/anos/crates/anos-llm-anthropic/src/provider.rs`.

/// ATD id → MCP-safe name. Two-way reversible: `:` → `_` and `.` → `_`.
pub fn sanitize(atd_id: &str) -> String {
    atd_id.replace(':', "_").replace('.', "_")
}

/// MCP name → ATD id. Recognizes the `<namespace>_` prefix and splits the rest.
/// Falls back to returning the name unchanged if the shape isn't recognized.
pub fn desanitize(mcp_name: &str) -> String {
    // Known namespaces used by the ANOS daemon.
    for ns in &["anos", "host", "mock"] {
        let prefix = format!("{ns}_");
        if let Some(rest) = mcp_name.strip_prefix(&prefix) {
            if let Some((domain, action)) = rest.split_once('_') {
                return format!("{ns}:{domain}.{}", action.replace('_', "."));
            }
            return format!("{ns}:{rest}");
        }
    }
    mcp_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_colon_and_dot() {
        assert_eq!(sanitize("anos:fs.read"), "anos_fs_read");
        assert_eq!(sanitize("host:media.convert"), "host_media_convert");
    }

    #[test]
    fn desanitize_recovers_id_for_anos_namespace() {
        assert_eq!(desanitize("anos_fs_read"), "anos:fs.read");
        assert_eq!(desanitize("anos_system_time"), "anos:system.time");
        assert_eq!(desanitize("host_media_convert"), "host:media.convert");
    }

    #[test]
    fn desanitize_handles_variant_suffixes() {
        assert_eq!(desanitize("anos_fs_read_bytes"), "anos:fs.read.bytes");
    }

    #[test]
    fn desanitize_passthroughs_unknown_namespace() {
        assert_eq!(desanitize("weird_tool_name"), "weird_tool_name");
    }

    #[test]
    fn sanitize_desanitize_roundtrip_for_known_ids() {
        for id in &[
            "anos:fs.read",
            "anos:web.search",
            "anos:shell.exec",
            "host:media.convert",
            "mock:echo.say",
        ] {
            let s = sanitize(id);
            let back = desanitize(&s);
            assert_eq!(&back, id, "roundtrip failed for {id}: sanitized={s}");
        }
    }
}
