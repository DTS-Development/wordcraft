//! Preferences between visits (#2132): the same [`UiState`] the desktop app keeps in `ui.json`
//! (theme, interface size, recent symbols, editing options…), kept in the browser's
//! `localStorage` under [`KEY`]. Restored at startup; written back whenever it changes, so a
//! reload or a closed tab keeps the last setting (browsers don't reliably run exit code).

use wordcraft_ui_egui::{UiState, WordApp};

/// The `localStorage` key.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub const KEY: &str = "wordcraft.ui";
/// Largest stored value read back (`localStorage` holds about 5 MB per site).
const MAX_STORED: usize = 4 << 20;

/// Apply preferences stored on an earlier visit. Missing, oversized or unreadable values leave
/// the defaults. `host`: PPM's page owns the files, so no recent file names are taken back, and the
/// interface language is the page's, not the browser's or the stored one: `lang` (the page's
/// `?lang=` value) when it names a language WordCraft has, else Korean. Without `host`, `lang` is
/// not read.
pub fn restore(app: &mut WordApp, stored: Option<&str>, host: bool, lang: Option<&str>) {
    if let Some(s) = stored.filter(|s| s.len() <= MAX_STORED)
        && let Some(ui) = UiState::from_json(s.as_bytes())
    {
        app.apply_prefs(ui);
    }
    if host {
        app.ui.recent.clear();
        app.ui.language = host_language(lang).into();
    }
}

/// The interface language of the PPM (host) build when its page names none (or one WordCraft
/// lacks).
const HOST_LANGUAGE: &str = "ko";

/// The PPM build's interface language for the page's `?lang=` value: a registered language code
/// (any case), else [`HOST_LANGUAGE`]. `auto` is not taken: the page decides, not the browser.
fn host_language(lang: Option<&str>) -> &'static str {
    lang.and_then(wordcraft_ui_egui::i18n::normalize_pref).filter(|l| *l != wordcraft_ui_egui::i18n::AUTO).unwrap_or(HOST_LANGUAGE)
}

/// Tracks what was last written, so the store is written only when a preference changed.
#[derive(Default)]
pub struct PrefsSync {
    last: Option<String>,
    /// Host mode: file names never reach the store, and names stored earlier are written over.
    host: bool,
}

impl PrefsSync {
    /// Starts from what is already stored, so restoring doesn't write it straight back.
    pub fn new(stored: Option<String>) -> Self {
        Self { last: stored, host: false }
    }

    /// Keep recent file names out of the store ([`restore`]'s `host`).
    pub fn host(mut self, host: bool) -> Self {
        self.host = host;
        self
    }

    /// The JSON to store when the app's preferences differ from what was last written.
    pub fn changed(&mut self, app: &WordApp) -> Option<String> {
        let mut prefs = app.prefs();
        if self.host {
            prefs.recent.clear();
        }
        let json = match serde_json::to_string(&prefs) {
            Ok(j) => j,
            Err(e) => {
                log::warn!("preferences: cannot serialize: {e}");
                return None;
            }
        };
        if self.last.as_deref() == Some(json.as_str()) {
            return None;
        }
        self.last = Some(json.clone());
        Some(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wordcraft_ui_egui::Services;

    fn app() -> WordApp {
        WordApp::new(wordcraft_engine::Session::new(wordcraft_engine::sample::sample_document()), Services::default())
    }

    /// #2132: dark mode chosen on one visit is there after a reload.
    #[test]
    fn dark_mode_survives_reload() {
        let mut first = app();
        let mut sync = PrefsSync::new(None);
        let initial = sync.changed(&first);
        assert!(initial.is_some(), "first frame stores the preferences");
        assert_eq!(sync.changed(&first), None, "unchanged preferences are not rewritten");
        first.run("ui.theme", json!({"value": "dark"})).unwrap();
        let stored = sync.changed(&first).expect("the theme change is stored");

        let mut reloaded = app();
        restore(&mut reloaded, Some(&stored), false, None);
        assert_eq!(reloaded.ui.theme, wordcraft_ui_egui::theme::Appearance::Dark);
        assert_eq!(PrefsSync::new(Some(stored)).changed(&reloaded), None, "restoring doesn't write back");
    }

    #[test]
    fn junk_storage_keeps_defaults() {
        for junk in [None, Some(""), Some("not json"), Some("[1,2]"), Some("{\"theme\":42}")] {
            let mut a = app();
            restore(&mut a, junk, false, None);
            assert_eq!(a.ui.theme, wordcraft_ui_egui::theme::Appearance::System, "junk keeps the default appearance");
        }
    }

    /// PPM's host mode: recent file names are neither restored from nor written to the store, and
    /// a store that already holds some is written over at the first pass.
    #[test]
    fn host_mode_keeps_recent_file_names_out_of_the_store() {
        let mut desktop = app();
        desktop.ui.recent = vec!["/home/me/secret plan.docx".into()];
        let with_names = serde_json::to_string(&desktop.prefs()).unwrap();
        assert!(with_names.contains("secret plan"), "the fixture stores a name");

        let mut host = app();
        restore(&mut host, Some(&with_names), true, None);
        assert!(host.ui.recent.is_empty(), "nothing is restored");
        let mut sync = PrefsSync::new(Some(with_names)).host(true);
        let rewritten = sync.changed(&host).expect("the store held names, so it is written over");
        assert!(!rewritten.contains("secret plan"));
        // A name that shows up later is not written either.
        host.ui.recent = vec!["/x/late.docx".into()];
        assert_eq!(sync.changed(&host), None);
    }

    /// The PPM build starts in Korean whatever the browser or an earlier visit chose; the
    /// upstream build keeps following the system (`auto`) or the stored choice.
    #[test]
    fn host_mode_starts_in_korean() {
        let mut other = app();
        other.ui.language = "de".into();
        let stored = serde_json::to_string(&other.prefs()).unwrap();

        let mut host = app();
        restore(&mut host, Some(&stored), true, None);
        assert_eq!(host.ui.language, "ko");
        assert_eq!(wordcraft_ui_egui::i18n::Lang::from_pref(&host.ui.language).code(), "ko");
        let mut fresh = app();
        restore(&mut fresh, None, true, None);
        assert_eq!(fresh.ui.language, "ko");
        // A stored English (the visitor chose it in an earlier session) is ignored too.
        let mut english = app();
        english.ui.language = "en".into();
        let stored_en = serde_json::to_string(&english.prefs()).unwrap();
        let mut host_en = app();
        restore(&mut host_en, Some(&stored_en), true, None);
        assert_eq!(host_en.ui.language, "ko");

        // The page's `?lang=` picks the language; one WordCraft has no catalog for keeps Korean.
        for (asked, want) in [("ja", "ja"), ("en", "en"), ("EN", "en"), ("auto", "ko"), ("xx", "ko"), ("", "ko")] {
            let mut framed = app();
            restore(&mut framed, Some(&stored), true, Some(asked));
            assert_eq!(framed.ui.language, want, "?lang={asked}");
        }
        // The upstream build does not read it.
        let mut upstream_lang = app();
        restore(&mut upstream_lang, Some(&stored), false, Some("ja"));
        assert_eq!(upstream_lang.ui.language, "de");

        let mut upstream = app();
        restore(&mut upstream, Some(&stored), false, None);
        assert_eq!(upstream.ui.language, "de");
        let mut auto = app();
        restore(&mut auto, None, false, None);
        assert_eq!(auto.ui.language, wordcraft_ui_egui::i18n::AUTO);
    }
}
