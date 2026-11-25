//! The import loop shared by Liquid Tools and the backup `merge` (ARCHITECTURE §17):
//! folders first, then pages of ≈ 200 records → `Store::import_batch` (one transaction
//! each), `import:progress` after every page, the report at the end.
//!
//! With `settings.privacy.secretPatterns` on, a text record that looks like a secret (the
//! capture rule, `capture::secrets`) is not imported and counts as `skipped` (FIX-01) —
//! otherwise an import would bring in what the capture had refused.

use crate::capture::secrets;
use crate::logx;
use crate::model::{ImportReport, Kind};
use crate::store::bulk::{BatchStats, ImportItem, ImportSession};
use crate::store::{NewClip, Store};

/// Records per page / transaction.
pub(crate) const PAGE: i64 = 200;

/// `(phase, done, total)` → `import:progress`.
pub(crate) type Progress<'a> = &'a dyn Fn(&str, i64, i64);

/// One page read from a source; `Err` = a record that could not be read (counted, skipped).
/// `ImportItem.fav_order` holds the 0-based rank in the source's favorites order; the
/// runner turns it into an absolute position after the existing favorites.
pub(crate) type Page = Vec<Result<ImportItem, String>>;

/// A text record whose whole text looks like a card number, an API key or a private key.
/// Images and file lists are never secrets (as in the capture worker).
fn looks_like_secret(clip: &NewClip) -> bool {
    !matches!(clip.kind, Kind::Image | Kind::File) && clip.text.as_deref().is_some_and(secrets::looks_like_secret)
}

/// `skip_secrets` = `settings.privacy.secretPatterns`.
pub(crate) fn run(
    store: &Store,
    folders: &[String],
    total: i64,
    skip_secrets: bool,
    progress: Progress<'_>,
    mut next_page: impl FnMut() -> Result<Option<Page>, String>,
) -> Result<ImportReport, String> {
    let mut session = ImportSession::default();
    let nf = folders.len() as i64;
    progress("folders", 0, nf);
    let created = store.import_folders(folders, &mut session)?;
    progress("folders", nf, nf);
    let base = store.fav_order_base()?;
    let mut stats = BatchStats::default();
    let mut secret = 0i64;
    let mut done = 0i64;
    progress("clips", 0, total);
    while let Some(page) = next_page()? {
        done += page.len() as i64;
        let mut items = Vec::with_capacity(page.len());
        for rec in page {
            match rec {
                Ok(it) if skip_secrets && looks_like_secret(&it.clip) => secret += 1,
                Ok(mut it) => {
                    it.fav_order = it.fav_order.map(|rank| base + 1.0 + rank);
                    items.push(it);
                }
                Err(e) => {
                    stats.errors += 1;
                    logx::warn("import", &format!("record unreadable: {e}"));
                }
            }
        }
        stats += store.import_batch(items, &mut session)?;
        progress("clips", done, total.max(done));
    }
    stats.skipped += secret;
    logx::warn(
        "import",
        &format!(
            "done: {} imported, {} skipped ({secret} looked like secrets), {} errors, {created} folders",
            stats.imported, stats.skipped, stats.errors
        ),
    );
    Ok(ImportReport { imported: stats.imported, skipped: stats.skipped, folders: created, errors: stats.errors })
}
