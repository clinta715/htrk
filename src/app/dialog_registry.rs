//! Central registry of every dialog in the app (UI_BUG_AUDIT P2, kills F1.2/F3.1).
//!
//! The `dialog_registry!` invocation below is the SINGLE source of truth:
//! it generates the `Dialog` enum, `Dialog::ALL` (in Escape-close priority
//! order, topmost first), and the `is_open`/`close` accessors that hide each
//! dialog's storage shape (plain bool, `Option<T>`, or a sub-struct field).
//!
//! `HtrkApp::any_dialog_open()` and the Escape handler are both derived from
//! this list, so a dialog can no longer be silently missing from either —
//! the §11 "when adding a new dialog, add it to this list" rule is now
//! enforced by construction instead of by memory. Adding a dialog = adding
//! one line here (plus its storage field and draw code).

use super::HtrkApp;

/// Declares all dialogs from one list. Each entry is
/// `Variant = (is_open, close)` — closures over `&HtrkApp` / `&mut HtrkApp`.
/// Order matters: earlier entries close first on Escape (topmost first).
macro_rules! dialog_registry {
    ($($variant:ident = ($open:expr, $close:expr $(,)?),)+) => {
        /// Every dialog in the app. See `dialog_registry.rs`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Dialog {
            $($variant),+
        }

        impl Dialog {
            /// All dialogs, in Escape-close priority order (first = topmost).
            pub(crate) const ALL: &'static [Dialog] = &[$(Dialog::$variant),+];

            pub(crate) fn is_open(self, app: &HtrkApp) -> bool {
                match self {
                    $(Dialog::$variant => ($open as fn(&HtrkApp) -> bool)(app),)+
                }
            }

            pub(crate) fn close(self, app: &mut HtrkApp) {
                match self {
                    $(Dialog::$variant => ($close as fn(&mut HtrkApp))(app),)+
                }
            }
        }
    };
}

dialog_registry! {
    // Modal confirmations and simple info dialogs close first.
    ExitConfirm = (|app| app.show_exit_confirm, |app| app.show_exit_confirm = false),
    Shortcuts = (|app| app.show_shortcuts, |app| app.show_shortcuts = false),
    About = (|app| app.show_about, |app| app.show_about = false),
    // Tool dialogs.
    Settings = (|app| app.settings_state.open, |app| app.settings_state.open = false),
    PhraseGenerator = (|app| app.show_phrase_generator, |app| app.show_phrase_generator = false),
    Slice = (|app| app.slice_dialog_open, |app| app.slice_dialog_open = false),
    // Generator popups (typed bool fields on their panel structs, P3).
    AutomationGenerator = (
        |app| app.automation_editor.state.generator_open,
        |app| app.automation_editor.state.generator_open = false,
    ),
    EnvelopeGenerator = (
        |app| app.instrument_editor.envelope_generator_open,
        |app| app.instrument_editor.envelope_generator_open = false,
    ),
    SampleSelector = (
        |app| app.instrument_editor.sample_browser_open,
        |app| app.instrument_editor.sample_browser_open = false,
    ),
    // Browsers and export dialogs.
    FileBrowser = (|app| app.file_browser.show, |app| app.file_browser.show = false),
    WavExport = (|app| app.wav_export_state.open, |app| app.wav_export_state.open = false),
    SampleExport = (
        |app| app.sample_export_dialog.is_some(),
        |app| app.sample_export_dialog = None,
    ),
    // Plugin browsers were missing from the old hand-maintained Escape list
    // (Escape was dead while only one of these was open) — registered here.
    SendFxPluginBrowser = (
        |app| app.sendfx_panel.plugin_browser_open_for.is_some(),
        |app| app.sendfx_panel.plugin_browser_open_for = None,
    ),
    InstrumentPluginBrowser = (
        |app| app.instrument_editor.plugin_browser_open,
        |app| app.instrument_editor.plugin_browser_open = false,
    ),
    SampleLibrary = (|app| app.sample_library_state.open, |app| app.sample_library_state.open = false),
}

impl HtrkApp {
    /// The topmost open dialog in Escape-close priority order, if any.
    pub(crate) fn topmost_dialog(&self) -> Option<Dialog> {
        Dialog::ALL.iter().copied().find(|&d| d.is_open(self))
    }

    /// Close the topmost open dialog. Returns `true` if one was closed.
    pub(crate) fn close_topmost_dialog(&mut self) -> bool {
        match self.topmost_dialog() {
            Some(d) => {
                d.close(self);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> HtrkApp {
        HtrkApp::from_config_for_tests(crate::app_config::AppConfig::default())
    }

    #[test]
    fn no_dialog_open_by_default() {
        let app = test_app();
        assert!(app.topmost_dialog().is_none());
        assert!(!app.any_dialog_open());
    }

    #[test]
    fn each_dialog_opens_and_closes() {
        // (open it, assert topmost) — one arm per registry variant so a new
        // variant without a test arm fails to compile only if Dialog::ALL
        // coverage below is extended; the count assert catches omissions.
        let mut app = test_app();
        app.show_exit_confirm = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::ExitConfirm));
        assert!(app.close_topmost_dialog());
        assert!(!app.any_dialog_open());

        app.show_shortcuts = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::Shortcuts));
        assert!(app.close_topmost_dialog());

        app.show_about = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::About));
        assert!(app.close_topmost_dialog());

        app.settings_state.open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::Settings));
        assert!(app.close_topmost_dialog());

        app.show_phrase_generator = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::PhraseGenerator));
        assert!(app.close_topmost_dialog());

        app.slice_dialog_open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::Slice));
        assert!(app.close_topmost_dialog());

        app.automation_editor.state.generator_open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::AutomationGenerator));
        assert!(app.close_topmost_dialog());

        app.instrument_editor.envelope_generator_open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::EnvelopeGenerator));
        assert!(app.close_topmost_dialog());

        app.instrument_editor.sample_browser_open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::SampleSelector));
        assert!(app.close_topmost_dialog());

        app.file_browser.show = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::FileBrowser));
        assert!(app.close_topmost_dialog());

        app.wav_export_state.open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::WavExport));
        assert!(app.close_topmost_dialog());

        app.sample_export_dialog = Some(
            crate::ui::sample_export_dialog::SampleExportDialog::new(
                0,
                "test".to_string(),
                44100,
                None,
                16,
            ),
        );
        assert_eq!(app.topmost_dialog(), Some(Dialog::SampleExport));
        assert!(app.close_topmost_dialog());

        app.sendfx_panel.plugin_browser_open_for = Some(0);
        assert_eq!(app.topmost_dialog(), Some(Dialog::SendFxPluginBrowser));
        assert!(app.close_topmost_dialog());

        app.instrument_editor.plugin_browser_open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::InstrumentPluginBrowser));
        assert!(app.close_topmost_dialog());

        app.sample_library_state.open = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::SampleLibrary));
        assert!(app.close_topmost_dialog());

        assert!(!app.any_dialog_open());
    }

    #[test]
    fn all_variants_covered() {
        // Every registry variant is exercised by `each_dialog_opens_and_closes`.
        assert_eq!(Dialog::ALL.len(), 15);
    }

    #[test]
    fn priority_order_is_topmost_first() {
        let mut app = test_app();
        // A lower-priority dialog plus a higher-priority one: the earlier
        // registry entry wins.
        app.sample_library_state.open = true;
        app.show_exit_confirm = true;
        assert_eq!(app.topmost_dialog(), Some(Dialog::ExitConfirm));
        assert!(app.close_topmost_dialog());
        assert_eq!(app.topmost_dialog(), Some(Dialog::SampleLibrary));
        assert!(app.close_topmost_dialog());
        assert!(!app.any_dialog_open());
    }
}
