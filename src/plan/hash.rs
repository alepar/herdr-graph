//! Canonical JSON and `plan_hash` (spec §3.3).
use super::types::{Plan, PlanEffect};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Compact JSON with every object's keys sorted recursively (independent of serde_json's `preserve_order`).
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("string serializes"));
                out.push(':');
                write_canonical(&m[k], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).expect("scalar serializes")),
    }
}

/// hex sha256 of the canonical JSON of `{request:{kind,args}, relied_on, effects}`.
/// Commit oid, plan id, warnings, reserved-id table and timestamps are excluded.
pub fn plan_hash(p: &Plan) -> String {
    let v = serde_json::json!({
        "request": { "kind": p.request.kind, "args": p.request.args },
        "relied_on": p.relied_on,
        "effects": p.effects,
    });
    let digest = Sha256::digest(canonical_json(&v).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn sorted_keys(e: &[PlanEffect]) -> Vec<String> {
    let mut v: Vec<String> =
        e.iter().map(|x| canonical_json(&serde_json::to_value(x).expect("effect serializes"))).collect();
    v.sort();
    v
}

/// Effect-set equality for apply: the same multiset after sorting by canonical JSON.
pub fn same_effects(a: &[PlanEffect], b: &[PlanEffect]) -> bool {
    sorted_keys(a) == sorted_keys(b)
}
