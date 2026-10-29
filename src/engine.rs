//! Naming-engine selection: maps the `HERDR_NAMING_ENGINE` knob to an ordered
//! fallback chain. The cold phase tries each engine in turn and uses the first
//! that returns a slug, falling back to a deterministic local slug if all fail.
//!
//! Engines:
//! - `Lm`: a native OpenAI-compatible chat-completion call against a private
//!   LAN server, configured by `lm.json` in the plugin config dir. When that
//!   config exists it leads the default chain on every platform, since it is
//!   the operator's deliberate choice of naming backend.
//! - `Foundation`: on-device Apple FoundationModels via the `herdr-namer`
//!   Swift helper. macOS-only, compiled out entirely on other targets (the
//!   enum variant does not exist there, so a non-macOS build can neither
//!   select nor reference it).
//! - `Codex`: headless `codex exec` call.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Native OpenAI-compatible LAN LM call, configured via `lm.json`.
    Lm,
    /// On-device Apple FoundationModels via the `herdr-namer` Swift helper.
    #[cfg(target_os = "macos")]
    Foundation,
    /// Headless `codex exec` call.
    Codex,
}

/// Resolve the engine knob to the ordered list of engines to try.
///
/// - `lm`: LM only (skip every other engine, including on-device).
/// - `codex`: Codex only (skip the LM and on-device engines entirely).
/// - `foundation`: on-device first with Codex fallback, skipping the LM
///   engine (macOS only; elsewhere silently downgraded to the default chain).
/// - anything else (unset, empty, unknown): the platform default chain, with
///   `Lm` first when `lm_configured` reports a readable `lm.json`.
pub fn engine_chain(selection: Option<&str>, lm_configured: bool) -> Vec<Engine> {
    match selection.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("lm") => vec![Engine::Lm],
        Some("codex") => vec![Engine::Codex],
        #[cfg(target_os = "macos")]
        Some("foundation") => vec![Engine::Foundation, Engine::Codex],
        _ => default_chain(lm_configured),
    }
}

#[cfg(target_os = "macos")]
fn default_chain(lm_configured: bool) -> Vec<Engine> {
    if lm_configured {
        vec![Engine::Lm, Engine::Foundation, Engine::Codex]
    } else {
        vec![Engine::Foundation, Engine::Codex]
    }
}

#[cfg(not(target_os = "macos"))]
fn default_chain(lm_configured: bool) -> Vec<Engine> {
    if lm_configured {
        vec![Engine::Lm, Engine::Codex]
    } else {
        vec![Engine::Codex]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // An explicit selection is always honored, on every platform.
    #[test]
    fn lm_selection_is_lm_only() {
        assert_eq!(engine_chain(Some("lm"), true), vec![Engine::Lm]);
        assert_eq!(engine_chain(Some("lm"), false), vec![Engine::Lm]);
    }

    #[test]
    fn codex_selection_skips_lm_and_foundation() {
        assert_eq!(engine_chain(Some("codex"), true), vec![Engine::Codex]);
    }

    #[test]
    fn selection_is_case_and_whitespace_insensitive() {
        assert_eq!(engine_chain(Some("  LM "), true), vec![Engine::Lm]);
    }

    #[test]
    fn unknown_or_empty_falls_back_to_default_chain() {
        assert_eq!(engine_chain(Some("bogus"), false), default_chain(false));
        assert_eq!(engine_chain(Some(""), true), default_chain(true));
        assert_eq!(engine_chain(None, true), default_chain(true));
    }

    #[cfg(target_os = "macos")]
    mod macos {
        use super::*;

        #[test]
        fn default_chain_without_lm_is_foundation_then_codex() {
            assert_eq!(
                engine_chain(None, false),
                vec![Engine::Foundation, Engine::Codex]
            );
        }

        #[test]
        fn configured_lm_leads_the_default_chain() {
            assert_eq!(
                engine_chain(None, true),
                vec![Engine::Lm, Engine::Foundation, Engine::Codex]
            );
        }

        #[test]
        fn foundation_selection_keeps_codex_fallback_but_skips_lm() {
            assert_eq!(
                engine_chain(Some(" Foundation "), true),
                vec![Engine::Foundation, Engine::Codex]
            );
        }
    }

    // Off macOS there is no on-device engine: every selection that is not an
    // explicit `lm`/`codex` still resolves through the default chain, and
    // `foundation` is silently downgraded rather than attempted.
    #[cfg(not(target_os = "macos"))]
    mod non_macos {
        use super::*;

        #[test]
        fn default_chain_without_lm_is_codex_only() {
            assert_eq!(engine_chain(None, false), vec![Engine::Codex]);
        }

        #[test]
        fn configured_lm_leads_with_codex_fallback() {
            assert_eq!(engine_chain(None, true), vec![Engine::Lm, Engine::Codex]);
        }

        #[test]
        fn foundation_request_is_downgraded() {
            assert_eq!(engine_chain(Some("foundation"), false), vec![Engine::Codex]);
        }
    }
}
