//! View output: the `schema`/`head` envelope, the JSON-or-text choice and the shared instructions object.
use crate::model::{CommitId, Timestamp};
use serde::{Deserialize, Serialize};
use std::io::Write;

pub const SCHEMA: u32 = 1;

/// One view's body: serialized as the JSON contract, rendered as text from the same struct (spec §2.1).
///
/// Owns (with [`Envelope`], [`Format`] and [`emit`]): the `schema`/`head` envelope and the JSON-or-text
/// choice. Must not: hold logic beyond output selection, or build a view's data. Where new behavior goes:
/// a new view supplies one struct implementing `View` (its JSON is the struct, its text is `render_text`);
/// commands never hand-assemble the envelope or print a view themselves.
pub trait View: Serialize {
    fn render_text(&self, out: &mut String);
}

/// `{schema, head, live?, ...body}`. `head` is the invocation's pinned `CommitId` (40 hex), never a fresh
/// HEAD read; `live` is set by roster only.
#[derive(Debug, Serialize)]
pub struct Envelope<B> {
    /// Always [`SCHEMA`].
    pub schema: u32,
    pub head: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    #[serde(flatten)]
    pub body: B,
}

impl<B> Envelope<B> {
    pub fn new(head: &CommitId, body: B) -> Self {
        Envelope {
            schema: SCHEMA,
            head: head.0.clone(),
            live: None,
            body,
        }
    }

    pub fn with_live(mut self, live: bool) -> Self {
        self.live = Some(live);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

impl Format {
    pub fn from_flag(json: bool) -> Self {
        if json { Format::Json } else { Format::Text }
    }
}

/// The one emit step. Json: exactly one JSON document plus '\n' on `out`, each notice as a line on `err`.
/// Text: the view's text, then each notice as a line, all on `out`. Notices are facts with no JSON field
/// (the log pointer to `ops`, warnings about skipped records); a fact with a field is rendered by the view.
pub fn emit<B: View>(
    env: &Envelope<B>,
    format: Format,
    notices: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    match format {
        Format::Json => {
            serde_json::to_writer(&mut *out, env)?;
            out.write_all(b"\n")?;
            for n in notices {
                writeln!(err, "{n}")?;
            }
        }
        Format::Text => {
            let mut text = String::new();
            env.body.render_text(&mut text);
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            out.write_all(text.as_bytes())?;
            for n in notices {
                writeln!(out, "{n}")?;
            }
        }
    }
    Ok(())
}

/// `emit` to the process's stdout/stderr.
pub fn print<B: View>(
    env: &Envelope<B>,
    format: Format,
    notices: &[String],
) -> std::io::Result<()> {
    emit(
        env,
        format,
        notices,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// The per-clone `instructions` object of spec §2.2, used unchanged at roster `clones[].instructions`
/// (hg-blw.1) and stack `clones[].instructions` (hg-blw.2); hg-blw.12 fills its behavior. Views import it and
/// never define their own. It owns only the shape and its two constructors; the current/stale/unknown rule
/// belongs to LoadedInstructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstructionsView {
    pub status: InstructionsStatus,
    pub loaded_rev: Option<String>,
    pub loaded_at: Option<Timestamp>,
    pub loaded_session: Option<String>,
    /// LoadedInstructions' injection source word: "hook" | "hook-codex" (hg-blw.12).
    pub source: Option<String>,
    pub truncated: bool,
    /// `[]` with no record.
    pub parts: Vec<DeliveredPart>,
    /// `[]` with no record.
    pub omitted: Vec<String>,
    pub stale_parts: Vec<StalePart>,
    /// Set only for `error`.
    pub message: Option<String>,
    /// The spec §2.3 `note` slot, e.g. "codex session id form differs" or "hook predates --stdin-payload;
    /// rerun setup" (hg-blw.12 fills it).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionsStatus {
    Current,
    Stale,
    Unknown,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveredPart {
    pub label: String,
    pub delivered: Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Whole,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StalePart {
    pub label: String,
    pub change: PartChange,
    /// The 40-hex commit that last touched the part's path (`removed`: the path the loaded record names);
    /// absent beyond the history scan cap. Additive to the spec's `{label, change}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_last_touched: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartChange {
    Modified,
    Added,
    Removed,
}

impl InstructionsView {
    /// No record / phase 1: status unknown, every option null, every list empty, truncated false.
    pub fn unknown() -> Self {
        InstructionsView {
            status: InstructionsStatus::Unknown,
            loaded_rev: None,
            loaded_at: None,
            loaded_session: None,
            source: None,
            truncated: false,
            parts: vec![],
            omitted: vec![],
            stale_parts: vec![],
            message: None,
            note: None,
        }
    }

    /// A seat whose stack does not resolve (spec §2.1): status error with `message`.
    pub fn error(message: impl Into<String>) -> Self {
        InstructionsView {
            status: InstructionsStatus::Error,
            message: Some(message.into()),
            ..Self::unknown()
        }
    }
}

impl Default for InstructionsView {
    fn default() -> Self {
        Self::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::Fx;
    use crate::model::Lifecycle;
    use serde_json::{Value, json};

    #[derive(Serialize)]
    struct Rows {
        rows: Vec<u32>,
    }
    impl View for Rows {
        fn render_text(&self, out: &mut String) {
            out.push_str("rows text");
        }
    }

    fn real_commit() -> CommitId {
        let mut fx = Fx::new();
        fx.teamspace("alpha", Lifecycle::Active);
        fx.commit("c")
    }

    #[test]
    fn envelope_schema_and_head() {
        let c = real_commit();
        let v = serde_json::to_value(Envelope::new(&c, Rows { rows: vec![1, 2] })).unwrap();
        assert_eq!(v, json!({"schema": 1, "head": c.0, "rows": [1, 2]}));
        let head = v["head"].as_str().unwrap();
        assert!(
            head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit()),
            "{head}"
        );
        assert!(v.get("live").is_none());
        let v = serde_json::to_value(Envelope::new(&c, Rows { rows: vec![] }).with_live(false))
            .unwrap();
        assert_eq!(v["live"], json!(false));
    }

    #[test]
    fn emit_json_one_document() {
        let env = Envelope::new(&real_commit(), Rows { rows: vec![7] });
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(
            &env,
            Format::from_flag(true),
            &["pointer".into()],
            &mut out,
            &mut err,
        )
        .unwrap();
        let docs: Vec<Value> = serde_json::Deserializer::from_slice(&out)
            .into_iter::<Value>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert!(!String::from_utf8(out).unwrap().contains("pointer"));
        assert_eq!(String::from_utf8(err).unwrap(), "pointer\n");
    }

    #[test]
    fn emit_text_keeps_notices() {
        let env = Envelope::new(&real_commit(), Rows { rows: vec![] });
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(
            &env,
            Format::from_flag(false),
            &["pointer".into()],
            &mut out,
            &mut err,
        )
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "rows text\npointer\n");
        assert!(err.is_empty());
    }

    #[test]
    fn instructions_view_unknown_default() {
        assert_eq!(
            serde_json::to_value(InstructionsView::default()).unwrap(),
            json!({"status":"unknown","loaded_rev":null,"loaded_at":null,"loaded_session":null,
                   "source":null,"truncated":false,"parts":[],"omitted":[],"stale_parts":[],
                   "message":null,"note":null})
        );
        let e = serde_json::to_value(InstructionsView::error("x")).unwrap();
        assert_eq!(e["status"], json!("error"));
        assert_eq!(e["message"], json!("x"));
        assert_eq!(e["parts"], json!([]));
    }
}
