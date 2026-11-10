//! The page shown in the browser after the Google OAuth redirect.
//!
//! It is served from `127.0.0.1`, a different origin from the app, so it cannot read the app's
//! stored preferences. The backend mirrors the appearance prefs here (see `set_appearance`) and
//! injects them into the HTML template as JSON.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use std::sync::RwLock;

const TEMPLATE: &str = include_str!("../assets/oauth-callback.html");
const CONFIG_PLACEHOLDER: &str = "\"@@CONFIG@@\"";

pub const THEMES: &[&str] = &["light", "dark", "midnight"];
pub const COLORS: &[&str] = &["green", "blue", "red", "orange", "yellow"];

#[derive(Clone)]
pub struct Appearance {
    pub theme: String,
    pub color: String,
    pub lang: String,
    pub icon: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            color: "green".into(),
            lang: "en".into(),
            icon: super::commands::app_icon::DEFAULT_ID.into(),
        }
    }
}

static APPEARANCE: RwLock<Option<Appearance>> = RwLock::new(None);

pub fn normalize_theme(raw: &str) -> String {
    let raw = raw.trim().to_ascii_lowercase();
    THEMES.iter().find(|t| **t == raw).unwrap_or(&"light").to_string()
}

pub fn normalize_color(raw: &str) -> String {
    let raw = raw.trim().to_ascii_lowercase();
    COLORS.iter().find(|c| **c == raw).unwrap_or(&"green").to_string()
}

/// Mirrors the user's current appearance so the callback page can match it.
pub fn set_appearance(appearance: Appearance) {
    if let Ok(mut slot) = APPEARANCE.write() {
        *slot = Some(appearance);
    }
}

fn current_appearance() -> Appearance {
    APPEARANCE.read().ok().and_then(|a| a.clone()).unwrap_or_default()
}

#[derive(Serialize)]
struct PageConfig<'a> {
    state: &'a str,
    kind: &'a str,
    detail: &'a str,
    theme: &'a str,
    color: &'a str,
    lang: &'a str,
    icon: &'a str,
    #[serde(rename = "iconUrl")]
    icon_url: String,
}

pub enum Outcome<'a> {
    Success,
    /// `kind`: denied | state_missing | state_mismatch | code_missing. `detail` is shown verbatim.
    Error { kind: &'a str, detail: &'a str },
}

pub fn render(outcome: Outcome) -> String {
    render_with(outcome, &current_appearance())
}

fn render_with(outcome: Outcome, appearance: &Appearance) -> String {
    let (state, kind, detail) = match outcome {
        Outcome::Success => ("success", "", ""),
        Outcome::Error { kind, detail } => ("error", kind, detail),
    };
    let png = super::commands::app_icon::png_128(&appearance.icon);
    let config = PageConfig {
        state,
        kind,
        detail,
        theme: &appearance.theme,
        color: &appearance.color,
        lang: &appearance.lang,
        icon: &appearance.icon,
        icon_url: format!("data:image/png;base64,{}", STANDARD.encode(png)),
    };
    // `<` is escaped so no value can close the script element.
    let json = serde_json::to_string(&config)
        .unwrap_or_else(|_| "{}".into())
        .replace('<', "\\u003c");
    TEMPLATE.replacen(CONFIG_PLACEHOLDER, &json, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_config_and_escapes_script_breakout() {
        let html = render_with(
            Outcome::Error { kind: "denied", detail: "</script><b>" },
            &Appearance { theme: "midnight".into(), color: "blue".into(), lang: "de".into(), icon: "crt".into() },
        );
        assert!(!html.contains("@@CONFIG@@"));
        assert!(html.contains("\"theme\":\"midnight\""));
        assert!(html.contains("\"color\":\"blue\""));
        assert!(html.contains("\\u003c/script>\\u003cb>"));
        assert!(html.contains("data:image/png;base64,"));
    }

    #[test]
    fn unknown_values_fall_back() {
        assert_eq!(normalize_theme("neon"), "light");
        assert_eq!(normalize_color("pink"), "green");
        assert_eq!(normalize_color(" Blue "), "blue");
    }
}
