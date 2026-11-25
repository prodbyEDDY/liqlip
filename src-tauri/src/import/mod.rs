//! Import from Liquid Tools (read-only, idempotent, no screenshots / dictations) and
//! `.liqlip` backups (SPEC §4 "Импорт и экспорт"). Owner: **W2-B**. ARCHITECTURE §16.
//!
//! `lt` reads a copy of the Liquid Tools database, `backup` writes / reads the zip, `runner`
//! is the import loop both feed (`Store::import_batch`, `store/bulk.rs`), `commands` wires
//! the dialogs, `import:progress` and `clips:changed { origin: 'import' }`.

mod backup;
pub mod commands;
mod lt;
mod runner;
mod tmp;

#[cfg(test)]
mod tests;
