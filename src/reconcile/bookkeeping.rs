//! Binding and runtime write-back as bookkeeping mutations (no confirmation, spec §3.2). The reconciler
//! writes bindings it learned from creates; the observer remains the authority on availability.
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, Binding, Runtime};
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, IdKind};
use crate::ports::writer::{Writer, WriterError};
use crate::store::Record;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, Reject};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub const BINDING_KEY: &str = "bookkeeping.binding";
pub const RUNTIME_KEY: &str = "bookkeeping.runtime";

#[derive(Deserialize)]
struct BindingArgs {
    object: AnyId,
    binding: Binding,
    availability: Availability,
}

#[derive(Deserialize)]
struct RuntimeArgs {
    object: AnyId,
    availability: Availability,
    #[serde(default)]
    moved_out: Option<bool>,
}

fn bug(e: impl std::fmt::Display) -> MutationError {
    MutationError::Bug(e.to_string())
}

fn parse<T: DeserializeOwned>(cx: &MutationCx<'_>) -> Result<T, MutationError> {
    serde_json::from_value(cx.request.args.clone()).map_err(bug)
}

fn gone(object: &AnyId) -> MutationError {
    MutationError::Reject(Reject {
        reason: "object_missing".into(),
        explanation: format!("{object} does not exist at the committed head"),
        current_revs: vec![],
    })
}

/// Read the record of `object`, let `f` edit it, write it back (rev bumped by the overlay).
fn edit<R: Record>(
    cx: &mut MutationCx<'_>,
    object: &AnyId,
    f: impl FnOnce(&mut R, &mut MutationCx<'_>) -> Result<(), MutationError>,
) -> Result<(), MutationError> {
    let loc = cx.tree.locate(object)?.ok_or_else(|| gone(object))?;
    let mut rec: R = cx.tree.read_record(&loc.record_path)?.ok_or_else(|| gone(object))?;
    f(&mut rec, cx)?;
    cx.tree.put_record(loc.record_path, &mut rec)?;
    Ok(())
}

/// Apply `f` to the `runtime` of a teamspace, seat or clone (the seat flag hook gets the seat record).
fn with_runtime(
    cx: &mut MutationCx<'_>,
    object: &AnyId,
    moved_out: Option<bool>,
    f: impl Fn(&mut Runtime, &MutationCx<'_>),
) -> Result<(), MutationError> {
    match object.kind() {
        IdKind::Teamspace => edit::<TeamspaceRecord>(cx, object, |r, cx| {
            f(&mut r.runtime, cx);
            Ok(())
        }),
        IdKind::Seat => edit::<SeatRecord>(cx, object, |r, cx| {
            f(&mut r.runtime, cx);
            if let Some(m) = moved_out {
                r.moved_out = m;
            }
            Ok(())
        }),
        IdKind::Clone => edit::<CloneRecord>(cx, object, |r, cx| {
            f(&mut r.runtime, cx);
            Ok(())
        }),
        other => Err(bug(format!("{other:?} objects have no runtime"))),
    }
}

struct SetBinding;
impl Mutation for SetBinding {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: BindingArgs = parse(cx)?;
        with_runtime(cx, &a.object, None, |rt, cx| {
            rt.bound = Some(a.binding.clone());
            rt.availability = a.availability;
            rt.observed_at = Some(cx.now);
        })?;
        Ok(Applied { summary: format!("bind {}", a.object), action: None })
    }
}

struct SetRuntime;
impl Mutation for SetRuntime {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: RuntimeArgs = parse(cx)?;
        with_runtime(cx, &a.object, a.moved_out, |rt, cx| {
            rt.availability = a.availability;
            rt.observed_at = Some(cx.now);
        })?;
        Ok(Applied { summary: format!("runtime {}", a.object), action: None })
    }
}

/// Registers `bookkeeping.binding` and `bookkeeping.runtime`.
pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register(BINDING_KEY, Arc::new(SetBinding));
    reg.register(RUNTIME_KEY, Arc::new(SetRuntime));
}

fn request(args: serde_json::Value) -> ChangeRequest {
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
    }
}

/// Admit a binding write-back for `object`.
pub fn admit_binding(
    writer: &dyn Writer,
    object: &AnyId,
    binding: &Binding,
    availability: Availability,
) -> Result<(), WriterError> {
    writer
        .admit(request(serde_json::json!({
            "sub": "binding", "object": object, "binding": binding, "availability": availability,
        })))
        .map(|_| ())
}

/// Admit a runtime (availability, optionally `moved_out`) write-back for `object`.
pub fn admit_runtime(
    writer: &dyn Writer,
    object: &AnyId,
    availability: Availability,
    moved_out: Option<bool>,
) -> Result<(), WriterError> {
    writer
        .admit(request(serde_json::json!({
            "sub": "runtime", "object": object, "availability": availability, "moved_out": moved_out,
        })))
        .map(|_| ())
}
