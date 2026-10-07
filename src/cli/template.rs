//! Parsing/rendering of `plan template …` changes and template listings. Owned by hg-zmi.9.
//! Not flattened into the CLI root; if hg-zmi.9 needs a top-level command it adds one flatten line to cli/mod.rs.
use crate::model::template::{MemberSelector, Startup, TemplateRecord};
use crate::templates::document::TemplateDocument;
use std::path::Path;

/// Reads and validates a template document (`--from <file.toml>`). The daemon calls this: it runs on the same
/// machine as the user, and the stored plan carries the parsed document so it stays self-contained.
pub fn read_document(path: &Path) -> anyhow::Result<TemplateDocument> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    TemplateDocument::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// Human-readable template summary (used by `show`).
pub fn render_template(rec: &TemplateRecord) -> String {
    let mut out = format!("template {} ({}) rev {}\n", rec.name, rec.id, rec.rev);
    out.push_str(&format!("  kind: {:?}\n", rec.kind).to_lowercase());
    if let Some(c) = &rec.copied_from {
        out.push_str(&format!(
            "  copied from {} at {}\n",
            c.template,
            c.at.to_rfc3339()
        ));
    }
    let d = &rec.defaults;
    if d.harness.is_some() || d.model.is_some() || d.args.is_some() || d.summaries.is_some() {
        out.push_str(&format!(
            "  defaults: harness={} model={} args={} summaries={}\n",
            d.harness
                .map_or("-".to_owned(), |h| format!("{h:?}").to_lowercase()),
            d.model.as_deref().unwrap_or("-"),
            d.args.as_ref().map_or("-".to_owned(), |a| a.join(" ")),
            d.summaries.map_or("-".to_owned(), |s| s.to_string()),
        ));
    }
    out.push_str("  members:\n");
    for m in &rec.members {
        let startup = if m.startup == Startup::Active {
            "active"
        } else {
            "deferred"
        };
        out.push_str(&format!("    {} {} ({startup})\n", m.id, m.name));
        if let Some(id) = &m.seat_template {
            out.push_str(&format!("      seat template: {id}\n"));
        }
        if let Some(text) = &m.responsibility {
            out.push_str(&format!("      responsibility: {text}\n"));
        }
        if let Some(duty) = &m.system_duty {
            out.push_str(&format!("      system duty: {duty:?}\n"));
        }
    }
    if !rec.relationships.is_empty() {
        out.push_str("  relationships:\n");
        for r in &rec.relationships {
            let who = match &r.members {
                MemberSelector::All => "all members".to_owned(),
                MemberSelector::Ids(ids) => ids
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            };
            out.push_str(&format!("    thread {} <- {who}\n", r.thread));
        }
    }
    out
}
