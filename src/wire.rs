//! Input-side §7 payload — the slice of the wire the gate reads back.
//!
//! balls only ever SERIALIZES a payload to a plugin's stdin and never
//! deserializes one (the §7 no-return-channel rule), so this plugin owns the
//! matching deserialize for exactly the slice it needs: the `landing` (to find
//! this plugin's own config, §1/§12) and the op-start ball (`current_state`)
//! for the review's INTENT. Every other wire field is ignored by serde, which
//! keeps this type stable as the wire grows.
//!
//! Parsing is TOTAL: a malformed or future payload yields the default (empty
//! landing, no ball) rather than aborting — a chatty or evolving wire never
//! wedges a close, and the verdict still cannot false-pass (the ball is merely
//! described to the model as unavailable). Note core's `Task` marks the markdown
//! body `#[serde(skip)]`, so the body is NOT on the wire; the durable, author-set
//! `title`/`tags` — the anchor the grader checks the change against
//! (`completion-gate.md` §4) — are.

use serde::Deserialize;

/// The §7 fields the gate reads. `binding.landing` locates this plugin's config;
/// `current_state` is the op-start ball (`pre` payload, §7).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Wire {
    #[serde(default)]
    pub(crate) binding: WireBinding,
    #[serde(default)]
    pub(crate) current_state: Option<serde_json::Value>,
}

/// The one binding field the gate needs: the landing checkout path. Absent on a
/// degenerate payload → empty, which `load_config` reads as "use defaults".
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WireBinding {
    #[serde(default)]
    pub(crate) landing: String,
}

impl Wire {
    /// Parse the §7 payload JSON. Total: unparseable input is the default Wire.
    pub(crate) fn parse(payload: &str) -> Self {
        serde_json::from_str(payload).unwrap_or_default()
    }

    /// The ball's intent, rendered for the prompt. The op-start ball's
    /// `title`/`tags` are the durable anchor the grader checks against; an
    /// absent ball is an explicit placeholder, never a silent pass.
    pub(crate) fn ball_intent(&self) -> String {
        match &self.current_state {
            Some(value) => serde_json::to_string_pretty(value).unwrap_or_default(),
            None => "(ball metadata unavailable on the wire)".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_landing_and_ball_intent_and_ignores_extra_fields() {
        let w = Wire::parse(
            r#"{"op":"close","phase":"pre","actor":"x",
                "binding":{"landing":"/land","tasks_branch":"balls/tasks",
                           "store":"/s","invocation_path":"/p"},
                "current_state":{"title":"deliver the gate","tags":["x"]}}"#,
        );
        assert_eq!(w.binding.landing, "/land");
        let intent = w.ball_intent();
        assert!(intent.contains("deliver the gate"));
        assert!(intent.contains("\"x\""));
    }

    #[test]
    fn malformed_payload_is_the_default_wire() {
        let w = Wire::parse("not json at all");
        assert_eq!(w.binding.landing, "");
        assert_eq!(w.ball_intent(), "(ball metadata unavailable on the wire)");
    }

    #[test]
    fn absent_ball_is_an_explicit_placeholder() {
        let w = Wire::parse(r#"{"binding":{"landing":"/l"}}"#);
        assert_eq!(w.binding.landing, "/l");
        assert!(w.ball_intent().contains("unavailable"));
    }
}
