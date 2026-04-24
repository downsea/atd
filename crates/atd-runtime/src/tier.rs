//! Tier-aware dispatch policy.
//!
//! `atd_protocol::ToolTier` (Hot/Warm/Cold) is the authoritative tier enum —
//! this module adds the **policy** that maps each tier to a timeout budget
//! and a max-output budget used at dispatch time. SP-12 makes the `tier`
//! signal load-bearing; a future SP can extend this with placement /
//! priority semantics without changing the field on the definition.

use std::time::Duration;

pub use atd_protocol::ToolTier;

/// Per-tier budgets used when constructing `CallContext` for a tool call.
/// `Warm` defaults match the pre-SP-12 server config (1 MiB / 60 s) to keep
/// the 9 existing tools' behavior unchanged.
#[derive(Debug, Clone)]
pub struct TierPolicy {
    pub hot_timeout: Duration,
    pub warm_timeout: Duration,
    pub cold_timeout: Duration,
    pub hot_max_output: usize,
    pub warm_max_output: usize,
    pub cold_max_output: usize,
}

impl TierPolicy {
    /// Canonical defaults:
    /// - Hot: 500 ms / 64 KiB — latency-critical tools (sensors, cached state).
    /// - Warm: 5 s / 1 MiB — typical tool invocations (current server default).
    /// - Cold: 60 s / 16 MiB — long-running / large-output tools.
    pub fn defaults() -> Self {
        Self {
            hot_timeout: Duration::from_millis(500),
            warm_timeout: Duration::from_secs(5),
            cold_timeout: Duration::from_secs(60),
            hot_max_output: 64 * 1024,
            warm_max_output: 1024 * 1024,
            cold_max_output: 16 * 1024 * 1024,
        }
    }

    pub fn timeout(&self, tier: ToolTier) -> Duration {
        match tier {
            ToolTier::Hot => self.hot_timeout,
            ToolTier::Warm => self.warm_timeout,
            ToolTier::Cold => self.cold_timeout,
        }
    }

    pub fn max_output(&self, tier: ToolTier) -> usize {
        match tier {
            ToolTier::Hot => self.hot_max_output,
            ToolTier::Warm => self.warm_max_output,
            ToolTier::Cold => self.cold_max_output,
        }
    }

    /// Apply a single `"<tier>=<key>=<value>"` override. Keys: `timeout_ms`,
    /// `max_output_bytes`. Returns a human-readable error on malformed input
    /// so the CLI can surface it via exit 2.
    pub fn apply_override(&mut self, spec: &str) -> Result<(), String> {
        let parts: Vec<&str> = spec.splitn(3, '=').collect();
        if parts.len() != 3 {
            return Err(format!(
                "expected '<tier>=<key>=<value>', got '{spec}'"
            ));
        }
        let tier = match parts[0] {
            "hot" => ToolTier::Hot,
            "warm" => ToolTier::Warm,
            "cold" => ToolTier::Cold,
            other => return Err(format!("unknown tier '{other}' (want hot|warm|cold)")),
        };
        match parts[1] {
            "timeout_ms" => {
                let v: u64 = parts[2]
                    .parse()
                    .map_err(|e| format!("invalid timeout_ms '{}': {e}", parts[2]))?;
                let d = Duration::from_millis(v);
                match tier {
                    ToolTier::Hot => self.hot_timeout = d,
                    ToolTier::Warm => self.warm_timeout = d,
                    ToolTier::Cold => self.cold_timeout = d,
                }
            }
            "max_output_bytes" => {
                let v: usize = parts[2]
                    .parse()
                    .map_err(|e| format!("invalid max_output_bytes '{}': {e}", parts[2]))?;
                match tier {
                    ToolTier::Hot => self.hot_max_output = v,
                    ToolTier::Warm => self.warm_max_output = v,
                    ToolTier::Cold => self.cold_max_output = v,
                }
            }
            other => {
                return Err(format!(
                    "unknown key '{other}' (want timeout_ms|max_output_bytes)"
                ));
            }
        }
        Ok(())
    }
}

impl Default for TierPolicy {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Parse a tier hint from an optional string (e.g. a `tier` field on a tool
/// definition, which may be absent). Defaults to `Warm` on `None` or an
/// unrecognized value — this is the SP-12 back-compat behavior locked in by
/// spec §8 Q5. A future SP can flip unknown-values to a hard error once all
/// builtin tools opt in.
pub fn tier_from_opt_str(s: Option<&str>) -> ToolTier {
    match s.map(|v| v.to_ascii_lowercase()).as_deref() {
        Some("hot") => ToolTier::Hot,
        Some("cold") => ToolTier::Cold,
        _ => ToolTier::Warm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_from_opt_str_defaults_to_warm_on_none() {
        assert_eq!(tier_from_opt_str(None), ToolTier::Warm);
    }

    #[test]
    fn tier_from_opt_str_defaults_to_warm_on_unknown() {
        assert_eq!(tier_from_opt_str(Some("nebulous")), ToolTier::Warm);
    }

    #[test]
    fn tier_from_opt_str_parses_known_values_case_insensitive() {
        assert_eq!(tier_from_opt_str(Some("hot")), ToolTier::Hot);
        assert_eq!(tier_from_opt_str(Some("HOT")), ToolTier::Hot);
        assert_eq!(tier_from_opt_str(Some("cold")), ToolTier::Cold);
        assert_eq!(tier_from_opt_str(Some("Warm")), ToolTier::Warm);
    }

    #[test]
    fn defaults_match_current_server_warm_budget() {
        // Migration-safety pin: Warm timeout >= 5s and max_output >= 1 MiB so
        // existing tools (which carry no tier) keep working. This test is a
        // tripwire — intentionally relax it only when flipping Warm defaults.
        let p = TierPolicy::defaults();
        assert!(p.warm_timeout >= Duration::from_secs(5));
        assert!(p.warm_max_output >= 1024 * 1024);
    }

    #[test]
    fn timeout_lookup_by_tier() {
        let p = TierPolicy::defaults();
        assert_eq!(p.timeout(ToolTier::Hot), Duration::from_millis(500));
        assert_eq!(p.timeout(ToolTier::Warm), Duration::from_secs(5));
        assert_eq!(p.timeout(ToolTier::Cold), Duration::from_secs(60));
    }

    #[test]
    fn apply_override_timeout_ms() {
        let mut p = TierPolicy::defaults();
        p.apply_override("hot=timeout_ms=300").unwrap();
        assert_eq!(p.hot_timeout, Duration::from_millis(300));
        // Other tiers untouched.
        assert_eq!(p.warm_timeout, Duration::from_secs(5));
    }

    #[test]
    fn apply_override_max_output_bytes() {
        let mut p = TierPolicy::defaults();
        p.apply_override("cold=max_output_bytes=33554432").unwrap();
        assert_eq!(p.cold_max_output, 33_554_432);
    }

    #[test]
    fn apply_override_rejects_malformed_spec() {
        let mut p = TierPolicy::defaults();
        assert!(p.apply_override("no_equals").is_err());
        assert!(p.apply_override("hot=timeout_ms").is_err());
        assert!(p.apply_override("bogus=timeout_ms=100").is_err());
        assert!(p.apply_override("hot=bogus=100").is_err());
        assert!(p.apply_override("hot=timeout_ms=not_a_number").is_err());
    }
}
