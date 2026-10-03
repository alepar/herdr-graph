//! Undo candidates, previews, compensating ops, caller-pane adoption (spec §6). Owned by hg-zmi.10.
pub mod adopt;
pub mod candidates;
pub mod commands;
pub mod compensate;
pub mod preview;

#[cfg(test)]
mod tests;

pub use commands::{UndoDeps, register_commands};

use crate::model::change::RequestKind;
use crate::plan::kind::KindRegistry;
use std::sync::Arc;

/// Registers the `undo` kind. Hydration and template-edit undo run through `application retire` and
/// `template edit`, so those kinds (core + templates) must be registered first.
pub fn register_kinds(reg: &mut KindRegistry) {
    let app_retire = reg
        .get(RequestKind::ApplicationRetire)
        .expect("register the templates kinds before undo");
    let template_edit = reg
        .get(RequestKind::TemplateEdit)
        .expect("register the templates kinds before undo");
    reg.register(Arc::new(compensate::UndoKind {
        app_retire,
        template_edit,
    }));
}
