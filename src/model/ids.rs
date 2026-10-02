//! Prefixed ULID identifiers (spec §2.1).
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;

const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("id {0:?} does not start with {1:?}")]
    WrongPrefix(String, &'static str),
    #[error("id {0:?} has an unknown prefix")]
    UnknownPrefix(String),
    #[error("id {0:?} body is not a 26-char uppercase Crockford ULID")]
    BadBody(String),
}

fn validate_body(full: &str, body: &str) -> Result<(), IdError> {
    if body.len() != 26 || !body.chars().all(|c| CROCKFORD.contains(c)) {
        return Err(IdError::BadBody(full.to_owned()));
    }
    ulid::Ulid::from_string(body).map_err(|_| IdError::BadBody(full.to_owned()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    Teamspace,
    Seat,
    Clone,
    NativeSession,
    Template,
    Member,
    Application,
    Operation,
    Effect,
    Action,
    Plan,
    Transcript,
    Request,
}

impl IdKind {
    pub const ALL: [(IdKind, &'static str); 13] = [
        (IdKind::Teamspace, "ts_"),
        (IdKind::Seat, "st_"),
        (IdKind::Clone, "cl_"),
        (IdKind::NativeSession, "ns_"),
        (IdKind::Template, "tpl_"),
        (IdKind::Member, "mem_"),
        (IdKind::Application, "app_"),
        (IdKind::Operation, "op_"),
        (IdKind::Effect, "ef_"),
        (IdKind::Action, "act_"),
        (IdKind::Plan, "pl_"),
        (IdKind::Transcript, "tr_"),
        (IdKind::Request, "rq_"),
    ];
    pub fn prefix(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(k, _)| *k == self)
            .map(|(_, p)| *p)
            .expect("all kinds listed")
    }
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $kind:expr, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        #[allow(clippy::new_without_default)]
        impl $name {
            pub const PREFIX: &'static str = $prefix;
            pub const KIND: IdKind = $kind;
            /// Mint a fresh id (time-ordered ULID).
            pub fn new() -> Self { Self::from_ulid(ulid::Ulid::new()) }
            pub fn from_ulid(u: ulid::Ulid) -> Self { Self(format!("{}{}", $prefix, u)) }
            pub fn parse(s: &str) -> Result<Self, IdError> {
                let body = s.strip_prefix($prefix).ok_or_else(|| IdError::WrongPrefix(s.to_owned(), $prefix))?;
                validate_body(s, body)?;
                Ok(Self(s.to_owned()))
            }
            pub fn as_str(&self) -> &str { &self.0 }
            /// Last 6 characters: used for slug collisions (`-<id6>`) and nonce labels (`·<ef6>`).
            pub fn suffix6(&self) -> &str { &self.0[self.0.len() - 6..] }
            pub fn to_any(&self) -> AnyId { AnyId(self.0.clone()) }
        }
        impl fmt::Display for $name { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) } }
        impl fmt::Debug for $name { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) } }
        impl std::str::FromStr for $name { type Err = IdError; fn from_str(s: &str) -> Result<Self, IdError> { Self::parse(s) } }
        impl Serialize for $name { fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(&self.0) } }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }
        impl From<$name> for AnyId { fn from(v: $name) -> AnyId { AnyId(v.0) } }
        impl TryFrom<AnyId> for $name { type Error = IdError; fn try_from(v: AnyId) -> Result<Self, IdError> { Self::parse(&v.0) } }
    };
}

define_id!(
    /// `ts_` teamspace.
    TeamspaceId, IdKind::Teamspace, "ts_"
);
define_id!(
    /// `st_` seat.
    SeatId, IdKind::Seat, "st_"
);
define_id!(
    /// `cl_` clone.
    CloneId, IdKind::Clone, "cl_"
);
define_id!(
    /// `ns_` native session record.
    NsId, IdKind::NativeSession, "ns_"
);
define_id!(
    /// `tpl_` template.
    TemplateId, IdKind::Template, "tpl_"
);
define_id!(
    /// `mem_` template member.
    MemberId, IdKind::Member, "mem_"
);
define_id!(
    /// `app_` application.
    AppId, IdKind::Application, "app_"
);
define_id!(
    /// `op_` operation.
    OpId, IdKind::Operation, "op_"
);
define_id!(
    /// `ef_` effect; deterministic, see [`EffectId::derive`].
    EffectId, IdKind::Effect, "ef_"
);
define_id!(
    /// `act_` grouped action.
    ActionId, IdKind::Action, "act_"
);
define_id!(
    /// `pl_` plan.
    PlanId, IdKind::Plan, "pl_"
);
define_id!(
    /// `tr_` transcript.
    TranscriptId, IdKind::Transcript, "tr_"
);
define_id!(
    /// `rq_` processing request.
    RequestId, IdKind::Request, "rq_"
);

impl EffectId {
    /// Effect identity `ef_ = hash(op_id, object_id, effect_kind, object_rev)` (spec §4.4).
    /// sha256 over the NUL-separated fields; the first 16 bytes become a ULID-shaped body.
    pub fn derive(op: &OpId, object: &AnyId, kind: &str, object_rev: u64) -> Self {
        let mut h = Sha256::new();
        h.update(op.as_str().as_bytes());
        h.update([0]);
        h.update(object.as_str().as_bytes());
        h.update([0]);
        h.update(kind.as_bytes());
        h.update([0]);
        h.update(object_rev.to_be_bytes());
        let digest = h.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Self::from_ulid(ulid::Ulid::from_bytes(bytes))
    }
}

/// Any graph id; carries its kind in its prefix. Used where a field may reference several object kinds.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnyId(String);

impl AnyId {
    pub fn parse(s: &str) -> Result<Self, IdError> {
        // All prefixes end in '_' and none is a prefix of another.
        for (_, p) in IdKind::ALL {
            if let Some(body) = s.strip_prefix(p) {
                validate_body(s, body)?;
                return Ok(Self(s.to_owned()));
            }
        }
        Err(IdError::UnknownPrefix(s.to_owned()))
    }
    pub fn kind(&self) -> IdKind {
        IdKind::ALL
            .iter()
            .find(|(_, p)| self.0.starts_with(p))
            .map(|(k, _)| *k)
            .expect("validated on construction")
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn suffix6(&self) -> &str {
        &self.0[self.0.len() - 6..]
    }
}

impl fmt::Display for AnyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl fmt::Debug for AnyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for AnyId {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, IdError> {
        Self::parse(s)
    }
}
impl Serialize for AnyId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for AnyId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
