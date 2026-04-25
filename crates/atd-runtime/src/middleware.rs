//! Result-middleware pipeline.
//!
//! A `Middleware` is invoked **on success** after a tool returns, with a
//! mutable reference to the result value. SP-12 ships one built-in
//! (`RedactPathsMiddleware`) to demonstrate the shape; the v3 brief's full
//! suite (pii_redact, source_device_tag, compress, audit_log, rate_shape)
//! is deferred.
//!
//! Error paths bypass middleware in SP-12 — spec §8 Q4. A future SP can
//! add an `on_error` hook once a real consumer exists.

use atd_protocol::ToolDefinition;

/// A result-rewriting hook. Must be deterministic and side-effect-free
/// beyond the `result` mutation + any internal audit sinks the impl owns.
pub trait Middleware: Send + Sync {
    fn name(&self) -> &'static str;

    fn on_result(&self, tool_id: &str, tool_def: &ToolDefinition, result: &mut serde_json::Value);
}

/// Walk a JSON value, applying `f` to every string leaf (including strings
/// inside arrays and nested objects). Non-string leaves are untouched.
fn walk_strings(value: &mut serde_json::Value, f: &mut impl FnMut(&mut String)) {
    match value {
        serde_json::Value::String(s) => f(s),
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                walk_strings(v, f);
            }
        }
        serde_json::Value::Object(obj) => {
            for (_k, v) in obj.iter_mut() {
                walk_strings(v, f);
            }
        }
        _ => {}
    }
}

/// Redact absolute filesystem paths from tool output. Applies each
/// `(pattern, replacement)` pair in order to every string leaf in the
/// result. Default construction via `with_home_default()` redacts
/// `$HOME/...` paths — a low-effort demonstration of the pattern, not a
/// comprehensive PII scrubber.
pub struct RedactPathsMiddleware {
    patterns: Vec<(regex::Regex, String)>,
}

impl RedactPathsMiddleware {
    pub fn new(patterns: Vec<(regex::Regex, String)>) -> Self {
        Self { patterns }
    }

    /// Redact the current user's home directory. If `$HOME` is unset (rare
    /// on CI, possible in containers), returns a middleware with an empty
    /// pattern set rather than panicking — it becomes a no-op.
    pub fn with_home_default() -> Self {
        let patterns = match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => {
                // Escape regex metacharacters in the path before compiling.
                let escaped = regex::escape(&home);
                match regex::Regex::new(&escaped) {
                    Ok(re) => vec![(re, "<redacted:home>".to_string())],
                    Err(_) => vec![],
                }
            }
            _ => vec![],
        };
        Self { patterns }
    }
}

impl Middleware for RedactPathsMiddleware {
    fn name(&self) -> &'static str {
        "redact_paths"
    }

    fn on_result(
        &self,
        _tool_id: &str,
        _tool_def: &ToolDefinition,
        result: &mut serde_json::Value,
    ) {
        if self.patterns.is_empty() {
            return;
        }
        let patterns = &self.patterns;
        walk_strings(result, &mut |s| {
            for (re, rep) in patterns {
                *s = re.replace_all(s, rep.as_str()).into_owned();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{
        BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources, ToolSafety,
        ToolTrust, ToolVisibility, TrustLevel,
    };

    fn tool_def() -> ToolDefinition {
        ToolDefinition {
            id: "test:mw".into(),
            name: "mw".into(),
            description: "middleware test fixture".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: "test".into(),
                actions: vec![],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({}),
            }],
            safety: ToolSafety {
                level: SafetyLevel::Read,
                dry_run: false,
                side_effects: vec![],
                data_sensitivity: None,
            },
            resources: ToolResources {
                timeout_ms: 1000,
                max_concurrent: 1,
                rate_limit_per_min: None,
                estimated_tokens: None,
            },
            trust: ToolTrust {
                publisher: "test".into(),
                trust_level: TrustLevel::L0Unverified,
                signature: None,
            },
            visibility: ToolVisibility::Read,
            required_capabilities: vec![],
            tier: None,
            errors: vec![],
        }
    }

    fn mw_with(pattern: &str, rep: &str) -> RedactPathsMiddleware {
        let re = regex::Regex::new(pattern).unwrap();
        RedactPathsMiddleware::new(vec![(re, rep.to_string())])
    }

    #[test]
    fn redacts_pattern_in_top_level_string() {
        let mw = mw_with(r"/home/[^/]+", "<redacted>");
        let def = tool_def();
        let mut v = serde_json::json!({"path": "/home/alice/x.txt"});
        mw.on_result("test:mw", &def, &mut v);
        assert_eq!(v["path"], "<redacted>/x.txt");
    }

    #[test]
    fn redacts_in_nested_object() {
        let mw = mw_with(r"secret", "***");
        let def = tool_def();
        let mut v = serde_json::json!({
            "outer": {"inner": "this is a secret value"}
        });
        mw.on_result("t", &def, &mut v);
        assert_eq!(v["outer"]["inner"], "this is a *** value");
    }

    #[test]
    fn redacts_in_array_elements() {
        let mw = mw_with(r"password=\w+", "password=<redacted>");
        let def = tool_def();
        let mut v = serde_json::json!({
            "entries": ["password=hunter2", "normal line", "password=correct horse"]
        });
        mw.on_result("t", &def, &mut v);
        let arr = v["entries"].as_array().unwrap();
        assert_eq!(arr[0], "password=<redacted>");
        assert_eq!(arr[1], "normal line");
        assert_eq!(arr[2], "password=<redacted> horse");
    }

    #[test]
    fn leaves_non_string_leaves_untouched() {
        let mw = mw_with(r"\d+", "N");
        let def = tool_def();
        let mut v = serde_json::json!({
            "num": 42,
            "bool": true,
            "null": null,
            "str_with_num": "port 42"
        });
        mw.on_result("t", &def, &mut v);
        assert_eq!(v["num"], 42);
        assert_eq!(v["bool"], true);
        assert_eq!(v["null"], serde_json::Value::Null);
        assert_eq!(v["str_with_num"], "port N");
    }

    #[test]
    fn applies_multiple_patterns_in_order() {
        let p1 = (regex::Regex::new(r"aaa").unwrap(), "bbb".to_string());
        let p2 = (regex::Regex::new(r"bbb").unwrap(), "ccc".to_string());
        // First 'aaa' -> 'bbb', then 'bbb' -> 'ccc'. End state: 'ccc'.
        let mw = RedactPathsMiddleware::new(vec![p1, p2]);
        let def = tool_def();
        let mut v = serde_json::json!({"x": "aaa"});
        mw.on_result("t", &def, &mut v);
        assert_eq!(v["x"], "ccc");
    }

    #[test]
    fn name_is_stable() {
        let mw = RedactPathsMiddleware::new(vec![]);
        assert_eq!(mw.name(), "redact_paths");
    }

    #[test]
    fn empty_middleware_is_a_noop() {
        let mw = RedactPathsMiddleware::new(vec![]);
        let def = tool_def();
        let mut v = serde_json::json!({"x": "unchanged"});
        mw.on_result("t", &def, &mut v);
        assert_eq!(v["x"], "unchanged");
    }

    #[test]
    fn with_home_default_handles_home_path_or_is_noop_when_unset() {
        // SAFETY-ish: we mutate HOME just for this test; other tests do not
        // rely on it. If HOME was unset to begin with, the middleware must
        // still be constructed without panic and act as a no-op.
        let prev = std::env::var_os("HOME");
        // Case A: HOME set.
        unsafe {
            std::env::set_var("HOME", "/tmp/fakehome-sp12");
        }
        let mw = RedactPathsMiddleware::with_home_default();
        let def = tool_def();
        let mut v = serde_json::json!({"p": "/tmp/fakehome-sp12/secret"});
        mw.on_result("t", &def, &mut v);
        assert_eq!(v["p"], "<redacted:home>/secret");

        // Case B: HOME unset.
        unsafe {
            std::env::remove_var("HOME");
        }
        let mw2 = RedactPathsMiddleware::with_home_default();
        let mut v2 = serde_json::json!({"p": "/tmp/anything"});
        mw2.on_result("t", &def, &mut v2);
        // No-op.
        assert_eq!(v2["p"], "/tmp/anything");

        // Restore HOME.
        if let Some(h) = prev {
            unsafe {
                std::env::set_var("HOME", h);
            }
        }
    }
}
