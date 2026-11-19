//! Balance kit and patches (docs/BALANCE_APP_SPEC.md): what both the server (`cargo xtask balance patch`) and the
//! app for designers need. The game schema (`Configure.pb`, `.bytes`) is deliberately not here: it exists only on
//! the server side, and the app works with the ready values of the kit.
//!
//! The patch check rules live here and are duplicated in `balance_kit/tools/check.py` for the LLM path without the app.
//! Both implementations run on the shared examples `tests/fixtures/cases.json`.

mod archive;
mod check;
mod edit;
mod kit;
pub mod loot;
mod loot_screen;
mod patch;
mod rebase;
mod screens;
mod values;
mod view;

pub use archive::{image_type, import, Imported, KitError, Limits};
pub use check::{check, Expect, Issue, Report, Severity, TableData};
pub use edit::{patch_file_name, DRAFT_FORMAT};
pub use kit::{Kit, CATALOG_FORMAT, KIT_FORMAT};
pub use patch::{Patch, PatchChange, PATCH_FORMAT};
pub use rebase::draft_fits;
pub use screens::{roll_json, BulkSpec};
pub use values::{as_number, editable, fmt_range, generic, is_int_text, ranges, same, wildcard, FieldKind, Value};
pub use view::{KitView, Query};
