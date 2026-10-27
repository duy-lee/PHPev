//! Text the daemon shows itself (tray menu, balloons, API messages), from the
//! same `i18n/<lang>.json` files as the dashboard.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use phpev_core::config::Config;
use phpev_core::layout::Layout;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

/// Languages with a strings file, first = fallback.
pub const LANGUAGES: &[&str] = &["en", "vi"];

static LANG: RwLock<&'static str> = RwLock::new("en");

/// `[ui] language` if set, otherwise the Windows display language.
pub fn resolve(choice: Option<&str>) -> &'static str {
    if let Some(lang) = choice.and_then(|c| LANGUAGES.iter().find(|l| **l == c)) {
        return lang;
    }
    // Primary language id 0x2A = Vietnamese.
    if unsafe { GetUserDefaultUILanguage() } & 0x3FF == 0x2A {
        "vi"
    } else {
        "en"
    }
}

/// Reads the choice from config.toml (at start and after it changes).
pub fn load(layout: &Layout) {
    let choice = Config::load(&layout.config_file()).ok().and_then(|c| c.ui.language);
    *LANG.write().unwrap_or_else(|e| e.into_inner()) = resolve(choice.as_deref());
}

pub fn current() -> &'static str {
    *LANG.read().unwrap_or_else(|e| e.into_inner())
}

fn strings(lang: &str) -> Option<&'static HashMap<String, String>> {
    static TABLES: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            LANGUAGES
                .iter()
                .filter_map(|l| {
                    let (bytes, _) = crate::dashboard::assets::lookup(&format!("/i18n/{l}.json"))?;
                    Some((*l, serde_json::from_slice(bytes).ok()?))
                })
                .collect()
        })
        .get(lang)
}

/// The text of `key` in the current language with `{name}` placeholders
/// filled in; English when missing, the key itself as a last resort.
pub fn tr(key: &str, vars: &[(&str, &dyn std::fmt::Display)]) -> String {
    let text = [current(), "en"]
        .iter()
        .find_map(|l| strings(l)?.get(key))
        .map_or(key, String::as_str);
    let mut out = text.to_owned();
    for (name, value) in vars {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key() {
        let en = strings("en").expect("en.json");
        for lang in LANGUAGES {
            let table = strings(lang).unwrap_or_else(|| panic!("{lang}.json is missing or invalid"));
            let missing: Vec<_> = en.keys().filter(|k| !table.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{lang}.json misses {missing:?}");
            let extra: Vec<_> = table.keys().filter(|k| !en.contains_key(*k)).collect();
            assert!(extra.is_empty(), "{lang}.json has unknown keys {extra:?}");
            for (key, text) in en {
                assert_eq!(
                    placeholders(text),
                    placeholders(&table[key]),
                    "{lang}.json: `{key}` has other placeholders"
                );
            }
        }
    }

    fn placeholders(text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = text
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .filter(|name| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(tr("msg.linked", &[("name", &"shop")]), "Linked shop");
        assert_eq!(tr("no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn known_choice_wins() {
        assert_eq!(resolve(Some("vi")), "vi");
        assert_eq!(resolve(Some("en")), "en");
    }
}
