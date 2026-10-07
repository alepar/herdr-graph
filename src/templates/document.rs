//! The TOML document format of a template (`template create/edit --from <file>`; also the shipped
//! `templates/*/template.toml` content): a `TemplateRecord` without the writer-owned bookkeeping.
use crate::model::common::SystemDuty;
use crate::model::template::{
    MemberDefaults, Relationship, Startup, TemplateKind, TemplateMember, TemplateRecord,
};
use crate::model::{MemberId, SCHEMA_VERSION, TemplateId};
use crate::plan::types::Reserved;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateDocument {
    pub name: String,
    #[serde(default)]
    pub kind: TemplateKind,
    /// Reusable instructions, stored as this template folder's AGENTS.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_md: Option<String>,
    #[serde(default)]
    pub defaults: MemberDefaults,
    #[serde(default)]
    pub members: Vec<DocumentMember>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentMember {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<MemberId>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_template: Option<TemplateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "role")]
    pub system_duty: Option<SystemDuty>,
    pub startup: Startup,
    #[serde(default)]
    pub defaults: MemberDefaults,
    /// Written to `templates/<slug>/members/<member-slug>/AGENTS.md`; `None` leaves an existing file alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_md: Option<String>,
}

/// One changed field of a template: `before`/`after` are `None` when the field was added/removed.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl TemplateDocument {
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: Self = toml::from_str(text).map_err(|e| format!("template document: {e}"))?;
        doc.validate()?;
        Ok(doc)
    }

    /// Member names are unique and non-empty; explicit member ids are unique.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("template name is empty".into());
        }
        if self.kind == TemplateKind::Seat
            && (!self.members.is_empty() || !self.relationships.is_empty())
        {
            return Err("a seat template cannot contain members or relationships".into());
        }
        let mut names = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for m in &self.members {
            if m.name.trim().is_empty() {
                return Err("a template member has an empty name".into());
            }
            if !names.insert(m.name.as_str()) {
                return Err(format!("duplicate member name {:?}", m.name));
            }
            if let Some(id) = &m.id
                && !ids.insert(id.clone())
            {
                return Err(format!("duplicate member id {id}"));
            }
        }
        Ok(())
    }

    /// A record for this document. Members without an id take `member_ids[name]` when given (edit keeps
    /// the id of a same-named existing member), else a reserved id in slot `member:<name>`.
    pub fn to_record(
        &self,
        id: TemplateId,
        rev: u64,
        member_ids: &BTreeMap<String, MemberId>,
        reserved: &mut Reserved,
    ) -> TemplateRecord {
        let members = self
            .members
            .iter()
            .map(|m| TemplateMember {
                id: m
                    .id
                    .clone()
                    .or_else(|| member_ids.get(&m.name).cloned())
                    .unwrap_or_else(|| reserved.get_or_mint(&format!("member:{}", m.name))),
                name: m.name.clone(),
                seat_template: m.seat_template.clone(),
                responsibility: m.responsibility.clone(),
                system_duty: m.system_duty,
                startup: m.startup,
                defaults: m.defaults.clone(),
            })
            .collect();
        TemplateRecord {
            schema: SCHEMA_VERSION,
            id,
            rev,
            name: self.name.clone(),
            kind: self.kind,
            name_history: vec![],
            defaults: self.defaults.clone(),
            members,
            relationships: self.relationships.clone(),
            copied_from: None,
        }
    }

    pub fn from_record(rec: &TemplateRecord) -> Self {
        Self {
            name: rec.name.clone(),
            kind: rec.kind,
            agents_md: None,
            defaults: rec.defaults.clone(),
            members: rec
                .members
                .iter()
                .map(|m| DocumentMember {
                    id: Some(m.id.clone()),
                    name: m.name.clone(),
                    seat_template: m.seat_template.clone(),
                    responsibility: m.responsibility.clone(),
                    system_duty: m.system_duty,
                    startup: m.startup,
                    defaults: m.defaults.clone(),
                    agents_md: None,
                })
                .collect(),
            relationships: rec.relationships.clone(),
        }
    }

    pub fn to_table(&self) -> toml::Table {
        toml::Table::try_from(self).expect("a template document serializes to a TOML table")
    }

    /// Field-level differences, in a stable order. Paths: `name`, `defaults.<key>`, `members.<id>` (added or
    /// removed), `members.<id>.<field>`, `members.<id>.defaults.<key>`, `relationships`. Members are keyed by
    /// id (name when a side has no id); `agents_md` is compared only when the new side sets it.
    pub fn diff(old: &Self, new: &Self) -> Vec<FieldChange> {
        let mut out = Vec::new();
        field(
            &mut out,
            "name",
            Some(json!(old.name)),
            Some(json!(new.name)),
        );
        field(
            &mut out,
            "kind",
            Some(json!(old.kind)),
            Some(json!(new.kind)),
        );
        if new.agents_md.is_some() {
            field(
                &mut out,
                "agents_md",
                json_opt(&old.agents_md),
                json_opt(&new.agents_md),
            );
        }
        defaults_diff(&mut out, "defaults", &old.defaults, &new.defaults);
        let key = |m: &DocumentMember| {
            m.id.as_ref()
                .map_or_else(|| m.name.clone(), |i| i.to_string())
        };
        let olds: BTreeMap<String, &DocumentMember> =
            old.members.iter().map(|m| (key(m), m)).collect();
        let news: BTreeMap<String, &DocumentMember> =
            new.members.iter().map(|m| (key(m), m)).collect();
        for (k, o) in &olds {
            match news.get(k) {
                None => field(&mut out, &format!("members.{k}"), Some(json!(o.name)), None),
                Some(n) => {
                    let p = format!("members.{k}");
                    field(
                        &mut out,
                        &format!("{p}.name"),
                        Some(json!(o.name)),
                        Some(json!(n.name)),
                    );
                    field(
                        &mut out,
                        &format!("{p}.seat_template"),
                        json_opt(&o.seat_template),
                        json_opt(&n.seat_template),
                    );
                    field(
                        &mut out,
                        &format!("{p}.responsibility"),
                        json_opt(&o.responsibility),
                        json_opt(&n.responsibility),
                    );
                    field(
                        &mut out,
                        &format!("{p}.system_duty"),
                        json_opt(&o.system_duty),
                        json_opt(&n.system_duty),
                    );
                    field(
                        &mut out,
                        &format!("{p}.startup"),
                        json_opt(&Some(o.startup)),
                        json_opt(&Some(n.startup)),
                    );
                    defaults_diff(&mut out, &format!("{p}.defaults"), &o.defaults, &n.defaults);
                    if n.agents_md.is_some() && o.agents_md != n.agents_md {
                        field(
                            &mut out,
                            &format!("{p}.agents_md"),
                            json_opt(&o.agents_md),
                            json_opt(&n.agents_md),
                        );
                    }
                }
            }
        }
        for (k, n) in &news {
            if !olds.contains_key(k) {
                field(&mut out, &format!("members.{k}"), None, Some(json!(n.name)));
            }
        }
        if old.relationships != new.relationships {
            field(
                &mut out,
                "relationships",
                serde_json::to_value(&old.relationships).ok(),
                serde_json::to_value(&new.relationships).ok(),
            );
        }
        out
    }
}

fn json_opt<T: Serialize>(v: &Option<T>) -> Option<Value> {
    v.as_ref().and_then(|x| serde_json::to_value(x).ok())
}

fn field(out: &mut Vec<FieldChange>, path: &str, before: Option<Value>, after: Option<Value>) {
    if before != after {
        out.push(FieldChange {
            path: path.to_owned(),
            before,
            after,
        });
    }
}

fn defaults_diff(out: &mut Vec<FieldChange>, p: &str, old: &MemberDefaults, new: &MemberDefaults) {
    field(
        out,
        &format!("{p}.harness"),
        json_opt(&old.harness),
        json_opt(&new.harness),
    );
    field(
        out,
        &format!("{p}.model"),
        json_opt(&old.model),
        json_opt(&new.model),
    );
    field(
        out,
        &format!("{p}.args"),
        json_opt(&old.args),
        json_opt(&new.args),
    );
    field(
        out,
        &format!("{p}.summaries"),
        json_opt(&old.summaries),
        json_opt(&new.summaries),
    );
}
