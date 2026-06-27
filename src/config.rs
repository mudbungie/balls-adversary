//! Owner config for the gate — the plugin's OWN territory (`design.md` §1/§12,
//! `completion-gate.md` §1), never a balls core field. Model, reasoning effort,
//! diff budget, the review rubric, and the fail-mode all live here so the
//! capability stays severable: delete the file and the gate runs on the
//! defaults below; delete the plugin and delete this directory, core untouched.
//!
//! At close.pre the binary reads `<landing>/config/plugins/adversary/config.toml`
//! (the landing is the `balls/config` checkout `bl install` copies config into,
//! §6/§13). An absent file, an absent field, or an empty landing each falls back
//! to the committed defaults — there is no run-time surprise, only owner policy
//! overlaid on a safe default (fail-CLOSED, the latest Opus, a completion-
//! oriented rubric). There is no shipped sample to drift against — the defaults
//! below ARE the source of truth; an owner writes the file only to override a
//! knob.

use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Default review model: the latest Opus. Owner-overridable.
const DEFAULT_MODEL: &str = "claude-opus-4-8";
/// Default reasoning effort for the single review pass.
const DEFAULT_EFFORT: &str = "high";
/// Default ceiling on the diff fed to the model, in bytes (~one model call).
const DEFAULT_MAX_DIFF_BYTES: usize = 200_000;
/// Default wall-clock budget for the review subprocess, in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// Where the plugin's config lives, relative to the landing checkout (§1/§12).
pub(crate) const CONFIG_REL: &str = "config/plugins/adversary/config.toml";

/// The default, owner-editable review rubric. Completion-oriented and
/// deliberately NOT asking the model to re-attest checkable facts (tests,
/// coverage, the line cap) — the pre-commit hook proves those for free
/// (`completion-gate.md` §3). Judgment only; cite evidence, not confidence.
/// Kept textually in sync with the shipped sample config.
const DEFAULT_RUBRIC: &str = concat!(
    "Judge ONLY what judgment can settle; do not re-verify tests, coverage, or\n",
    "the line-length cap — the repo's pre-commit hook already proves those facts.\n",
    "\n",
    "Pass the change ONLY if ALL of the following hold; cite the specific diff\n",
    "hunk for each judgement rather than asserting confidence:\n",
    "1. The diff actually delivers what the ball's title/intent asks for — not a\n",
    "   weaker reading of it. Grade the whole chain: does the change address the\n",
    "   stated intent, not merely make some change?\n",
    "2. The change is internally coherent — no half-applied edit, dangling\n",
    "   reference, obviously broken control flow, or contradiction with the ball.\n",
    "3. Nothing in the diff looks like an unrelated regression smuggled in under\n",
    "   the ball's name.\n",
    "\n",
    "When in genuine doubt, FAIL.",
);

/// The plugin's config. `#[serde(default)]` overlays a partial owner file onto
/// the literal defaults below, so any absent field is the default, never an
/// error.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) model: String,
    pub(crate) effort: String,
    /// `false` = fail-CLOSED (default): abort the close when claude is
    /// unreachable. `true` = fail-open: allow it with a logged warning.
    pub(crate) fail_open: bool,
    pub(crate) max_diff_bytes: usize,
    pub(crate) timeout_secs: u64,
    pub(crate) rubric: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            effort: DEFAULT_EFFORT.to_string(),
            fail_open: false,
            max_diff_bytes: DEFAULT_MAX_DIFF_BYTES,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            rubric: DEFAULT_RUBRIC.to_string(),
        }
    }
}

/// Load the gate's config from the landing checkout. Total: an empty landing,
/// an unreadable/absent file, or a malformed file all resolve to the safe
/// defaults (a broken file is reported on stderr, then ignored) — config is
/// owner policy layered on a default, and a typo must never wedge every close.
pub(crate) fn load_config(landing: &str) -> Config {
    if landing.is_empty() {
        return Config::default();
    }
    let path = Path::new(landing).join(CONFIG_REL);
    let Ok(text) = fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str(&text) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("adversary: ignoring malformed {}: {err}", path.display());
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_dir() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("adversary-cfg-{}-{n}", std::process::id()));
        fs::create_dir_all(dir.join("config/plugins/adversary")).unwrap();
        dir
    }

    fn write_config(dir: &Path, body: &str) {
        fs::write(dir.join(CONFIG_REL), body).unwrap();
    }

    #[test]
    fn defaults_are_safe_and_complete() {
        let c = Config::default();
        assert_eq!(c.model, "claude-opus-4-8");
        assert_eq!(c.effort, "high");
        assert!(!c.fail_open); // fail-closed by default
        assert_eq!(c.max_diff_bytes, 200_000);
        assert_eq!(c.timeout_secs, 600);
        assert!(c.rubric.contains("Judge ONLY what judgment can settle"));
    }

    #[test]
    fn empty_landing_is_defaults() {
        assert_eq!(load_config("").model, "claude-opus-4-8");
    }

    #[test]
    fn absent_file_is_defaults() {
        let dir = unique_dir();
        fs::remove_file(dir.join(CONFIG_REL)).ok(); // ensure absent
        assert_eq!(load_config(dir.to_str().unwrap()).model, "claude-opus-4-8");
    }

    #[test]
    fn partial_file_overlays_on_defaults() {
        let dir = unique_dir();
        write_config(&dir, "model = \"sonnet\"\nfail_open = true\n");
        let c = load_config(dir.to_str().unwrap());
        assert_eq!(c.model, "sonnet"); // overridden
        assert!(c.fail_open); // overridden
        assert_eq!(c.effort, "high"); // defaulted
        assert_eq!(c.max_diff_bytes, 200_000); // defaulted
    }

    #[test]
    fn malformed_file_is_defaults() {
        let dir = unique_dir();
        write_config(&dir, "this is = not valid = toml");
        assert_eq!(load_config(dir.to_str().unwrap()).model, "claude-opus-4-8");
    }
}
