//! Effective desired structure of an application (spec §5): the seats its template calls for, minus the
//! members it excluded, mapped through its `member_map`, plus the seats added to it independently.
use crate::model::application::ApplicationRecord;
use crate::model::common::{Availability, Lifecycle, Runtime};
use crate::model::effective::{EffectiveSeatConfig, resolve};
use crate::model::seat::{SeatRecord, TemplateRef};
use crate::model::template::{Startup, TemplateMember, TemplateRecord};
use crate::model::{AppId, MemberId, SCHEMA_VERSION, SeatId, TeamspaceId, TemplateId};
use crate::ports::store::{ObjectLocation, StoreError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatSource {
    /// An existing seat the application maps to a template member (created by it, or reused).
    Mapped,
    /// A template member with no seat yet: hydration or live propagation creates one.
    Create,
    /// A seat added to the application independently of the template.
    Addition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredSeat {
    pub member: Option<MemberId>,
    pub seat: Option<SeatId>,
    pub name: String,
    pub startup: Startup,
    pub config: EffectiveSeatConfig,
    pub source: SeatSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveStructure {
    pub application: AppId,
    pub template: TemplateId,
    pub seats: Vec<DesiredSeat>,
    /// Template members this application excludes.
    pub excluded: Vec<MemberId>,
}

impl EffectiveStructure {
    pub fn to_create(&self) -> impl Iterator<Item = &DesiredSeat> {
        self.seats.iter().filter(|s| s.source == SeatSource::Create)
    }
}

pub(crate) fn read_seat_rec(
    tree: &dyn TreeRead,
    id: &SeatId,
) -> Result<Option<(ObjectLocation, SeatRecord)>, StoreError> {
    let Some(loc) = layout::locate(tree, &id.to_any())? else { return Ok(None) };
    Ok(read_toml::<SeatRecord>(tree, &loc.record_path)?.map(|r| (loc, r)))
}

pub(crate) fn read_template(
    tree: &dyn TreeRead,
    id: &TemplateId,
) -> Result<Option<(ObjectLocation, TemplateRecord)>, StoreError> {
    let Some(loc) = layout::locate(tree, &id.to_any())? else { return Ok(None) };
    Ok(read_toml::<TemplateRecord>(tree, &loc.record_path)?.map(|r| (loc, r)))
}

/// The seat record a template member's seat starts as (also the stand-in used to resolve its config).
pub fn member_seat_record(
    id: SeatId,
    tpl: &TemplateId,
    member: &TemplateMember,
    ts: &TeamspaceId,
    app: &AppId,
) -> SeatRecord {
    SeatRecord {
        schema: SCHEMA_VERSION,
        id,
        rev: 0,
        name: member.name.clone(),
        name_history: vec![],
        teamspace: ts.clone(),
        lifecycle: if member.startup == Startup::Active { Lifecycle::Active } else { Lifecycle::Dormant },
        retired: None,
        role: None,
        template_ref: Some(TemplateRef { template: tpl.clone(), member: member.id.clone() }),
        applications: vec![app.clone()],
        overrides: Default::default(),
        participation: Default::default(),
        activation: Default::default(),
        runtime: Runtime { availability: Availability::Absent, bound: None, observed_at: None },
        channel: Default::default(),
        reload_required: false,
        moved_out: false,
    }
}

/// Structure of `app` against the template currently committed in `tree`.
pub fn effective_structure(tree: &dyn TreeRead, app: &ApplicationRecord) -> Result<EffectiveStructure, StoreError> {
    let (_, tpl) = read_template(tree, &app.template)?.ok_or_else(|| StoreError::Corrupt {
        path: layout::application_record(&app.id).as_str().into(),
        reason: format!("application {} names missing template {}", app.id, app.template),
    })?;
    effective_structure_with(tree, &tpl, app)
}

/// Structure of `app` against `tpl` (which may be a not-yet-committed edit of its template).
pub fn effective_structure_with(
    tree: &dyn TreeRead,
    tpl: &TemplateRecord,
    app: &ApplicationRecord,
) -> Result<EffectiveStructure, StoreError> {
    let graph = layout::read_graph(tree)?;
    let mut seats = Vec::new();
    let mut excluded = Vec::new();
    for m in &tpl.members {
        if app.exclusions.contains(&m.id) {
            excluded.push(m.id.clone());
            continue;
        }
        match app.member_map.get(&m.id) {
            Some(seat_id) => {
                let Some((_, seat)) = read_seat_rec(tree, seat_id)? else { continue };
                if seat.lifecycle == Lifecycle::Retired {
                    continue;
                }
                let config = resolve(&graph.defaults, Some(tpl), Some(m), &seat);
                seats.push(DesiredSeat {
                    member: Some(m.id.clone()),
                    seat: Some(seat.id.clone()),
                    name: seat.name,
                    startup: m.startup,
                    config,
                    source: SeatSource::Mapped,
                });
            }
            None => {
                let stand_in = member_seat_record(SeatId::new(), &tpl.id, m, &app.teamspace, &app.id);
                let config = resolve(&graph.defaults, Some(tpl), Some(m), &stand_in);
                seats.push(DesiredSeat {
                    member: Some(m.id.clone()),
                    seat: None,
                    name: m.name.clone(),
                    startup: m.startup,
                    config,
                    source: SeatSource::Create,
                });
            }
        }
    }
    for id in &app.additions {
        let Some((_, seat)) = read_seat_rec(tree, id)? else { continue };
        if seat.lifecycle == Lifecycle::Retired {
            continue;
        }
        let member = seat.template_ref.as_ref().and_then(|r| tpl.members.iter().find(|m| m.id == r.member));
        let config = resolve(&graph.defaults, Some(tpl), member, &seat);
        let startup = if seat.lifecycle == Lifecycle::Active { Startup::Active } else { Startup::Deferred };
        seats.push(DesiredSeat {
            member: None,
            seat: Some(seat.id.clone()),
            name: seat.name,
            startup,
            config,
            source: SeatSource::Addition,
        });
    }
    Ok(EffectiveStructure { application: app.id.clone(), template: tpl.id.clone(), seats, excluded })
}
