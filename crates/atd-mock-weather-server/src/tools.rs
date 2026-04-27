//! Three mock weather tools with static canned responses.
//!
//! All three share the `mock:weather.*` namespace; visibility=Read; no
//! required_capabilities; tier=Hot. They never hit a real API — every
//! call returns the same fixed data for Shanghai, marked as canned in
//! the response payload.

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTier, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Tool};
use serde_json::{Value, json};

fn weather_def(id: &str, action: &str, description: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: description.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        capability: ToolCapability {
            domain: "weather".into(),
            actions: vec![action.into()],
            tags: vec!["mock".into(), "demo".into()],
            intent_examples: vec![],
        },
        input_schema: json!({"type": "object"}),
        output_schema: json!({}),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: None,
        },
        resources: ToolResources {
            timeout_ms: 1000,
            max_concurrent: 8,
            rate_limit_per_min: None,
            estimated_tokens: None,
        },
        trust: ToolTrust {
            publisher: "mock".into(),
            trust_level: TrustLevel::L0Unverified,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: Some(ToolTier::Hot),
        errors: vec![],
    }
}

pub struct WeatherNowTool {
    def: ToolDefinition,
}

impl WeatherNowTool {
    pub fn new() -> Self {
        Self {
            def: weather_def(
                "mock:weather.now",
                "now",
                "Current weather conditions for Shanghai (canned demo data; not a real service). Args: {}. Returns {location, temperature_c, condition, humidity_pct, wind_kph, observed_at, note}.",
            ),
        }
    }
}

impl Default for WeatherNowTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WeatherNowTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, _args: Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            Ok(json!({
                "location": "Shanghai",
                "temperature_c": 18,
                "condition": "partly_cloudy",
                "humidity_pct": 62,
                "wind_kph": 12,
                "observed_at": "2026-04-27T08:00:00+08:00",
                "note": "canned demo data — not a real weather service",
            }))
        })
    }
}

pub struct WeatherForecastHourlyTool {
    def: ToolDefinition,
}

impl WeatherForecastHourlyTool {
    pub fn new() -> Self {
        Self {
            def: weather_def(
                "mock:weather.forecast.hourly",
                "forecast.hourly",
                "Hourly forecast for Shanghai (canned demo data; not a real service). Args: {hours?: u32 (1..24, default 6)}. Returns Vec<{hour, temperature_c, condition, precipitation_pct}>.",
            ),
        }
    }

    fn build_forecast(hours: u32) -> Value {
        // Six-hour fixed slice; clamp inputs so callers requesting more
        // get the same six entries (we don't fabricate data we don't have).
        let canned: &[(i32, i32, &str, u32)] = &[
            (9, 17, "partly_cloudy", 5),
            (10, 19, "partly_cloudy", 5),
            (11, 21, "partly_cloudy", 10),
            (12, 22, "cloudy", 20),
            (13, 21, "cloudy", 30),
            (14, 20, "light_rain", 60),
        ];
        let n = (hours as usize).clamp(1, canned.len());
        let entries: Vec<Value> = canned
            .iter()
            .take(n)
            .map(|(h, t, c, p)| {
                json!({
                    "hour": h,
                    "temperature_c": t,
                    "condition": c,
                    "precipitation_pct": p,
                })
            })
            .collect();
        json!({
            "location": "Shanghai",
            "hours": entries,
            "note": "canned demo data — capped at 6 entries regardless of hours requested",
        })
    }
}

impl Default for WeatherForecastHourlyTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WeatherForecastHourlyTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, args: Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        let hours = args
            .get("hours")
            .and_then(Value::as_u64)
            .map(|h| h.min(24) as u32)
            .unwrap_or(6);
        Box::pin(async move { Ok(Self::build_forecast(hours)) })
    }
}

pub struct WeatherSummaryTool {
    def: ToolDefinition,
}

impl WeatherSummaryTool {
    pub fn new() -> Self {
        Self {
            def: weather_def(
                "mock:weather.summary",
                "summary",
                "One-line plain-language gloss of today's weather + best activity window. Useful when an agent only needs a single composable string. Canned demo data.",
            ),
        }
    }
}

impl Default for WeatherSummaryTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WeatherSummaryTool {
    pub const SUMMARY: &'static str = "Shanghai today: 17–22°C, partly cloudy with afternoon showers possible. Light wind. Best window for outdoor activity 9–11am.";
}

impl Tool for WeatherSummaryTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, _args: Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            Ok(json!({
                "summary": Self::SUMMARY,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_now_definition_has_expected_id_and_no_caps() {
        let t = WeatherNowTool::new();
        assert_eq!(t.definition().id, "mock:weather.now");
        assert!(t.definition().required_capabilities.is_empty());
        assert_eq!(t.definition().capability.domain, "weather");
        assert!(matches!(t.definition().visibility, ToolVisibility::Read));
    }

    #[test]
    fn weather_forecast_hourly_clamps_hours_to_six() {
        // Mock data has 6 canned entries; forecast clamps regardless of input.
        let small = WeatherForecastHourlyTool::build_forecast(2);
        let large = WeatherForecastHourlyTool::build_forecast(24);
        assert_eq!(small["hours"].as_array().unwrap().len(), 2);
        assert_eq!(large["hours"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn weather_summary_returns_string_under_200_chars() {
        let t = WeatherSummaryTool::new();
        assert_eq!(t.definition().id, "mock:weather.summary");
        // Sanity-check the canned summary is short — keeps the agent's
        // composed prompt under reasonable token length.
        assert!(
            WeatherSummaryTool::SUMMARY.len() < 200,
            "summary should stay under 200 chars; got {}: {}",
            WeatherSummaryTool::SUMMARY.len(),
            WeatherSummaryTool::SUMMARY,
        );
    }
}
