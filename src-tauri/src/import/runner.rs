//! The import loop shared by Liquid Tools and the backup `merge` (ARCHITECTURE §16.3):
//! folders first, then pages of ≈ 200 records → `Store::import_batch` (one transaction
//! each), `import:progress` after every page, the report at the end.

use crate::logx;
use crate::model::ImportReport;
use crate::store::bulk::{BatchStats, ImportItem, ImportSession};
use crate::store::Store;

/// Records per page / transaction.
pub(crate) const PAGE: i64 = 200;

/// `(phase, done, total)` → `import:progress`.
pub(crate) type Progress<'a> = &'a dyn Fn(&str, i64, i64);

/// One page read from a source; `Err` = a record that could not be read (counted, skipped).
/// `ImportItem.fav_order` holds the 0-based rank in the source's favorites order; the
/// runner turns it into an absolute position after the existing favorites.
pub(crate) type Page = Vec<Result<ImportItem, String>>;

pub(crate) fn run(
    store: &Store,
    folders: &[String],
    total: i64,
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
    let mut done = 0i64;
    progress("clips", 0, total);
    while let Some(page) = next_page()? {
        done += page.len() as i64;
        let mut items = Vec::with_capacity(page.len());
        for rec in page {
            match rec {
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
    logx::warn(
        "import",
        &format!("done: {} imported, {} skipped, {} errors, {created} folders", stats.imported, stats.skipped, stats.errors),
    );
    Ok(ImportReport { imported: stats.imported, skipped: stats.skipped, folders: created, errors: stats.errors })
}
