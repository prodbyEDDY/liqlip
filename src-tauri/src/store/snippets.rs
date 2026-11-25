//! Snippets (SPEC §3.10): saved text templates in manual order; search through
//! `snippets_fts` (trigram) with the same term rules as the feed. Placeholders are expanded
//! by the frontend (`src/lib/snippets/`), not here.

use super::feed::{fts_quote, parse_terms, split_terms};
use super::{db_err, is_unique_violation, Store};
use crate::content::TEXT_MAX_BYTES;
use crate::model::{Snippet, SnippetInput};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, TransactionBehavior};

pub(crate) const SNIPPET_NAME_MAX: usize = 100;
pub(crate) const SNIPPET_KEYWORD_MAX: usize = 64;

const SNIPPET_COLS: &str = "s.id, s.name, s.body, s.keyword, s.position, s.created_at, s.updated_at, s.use_count";

fn row_to_snippet(r: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: r.get(0)?,
        name: r.get(1)?,
        body: r.get(2)?,
        keyword: r.get(3)?,
        position: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        use_count: r.get(7)?,
    })
}

fn snippet_by_id(conn: &Connection, id: i64) -> Result<Snippet, String> {
    conn.query_row(&format!("SELECT {SNIPPET_COLS} FROM snippets s WHERE s.id = ?1"), [id], row_to_snippet)
        .optional()
        .map_err(db_err)?
        .ok_or_else(|| format!("not_found: snippet {id}"))
}

impl Store {
    /// Snippets matching `text` ("" = all), in manual order.
    pub(crate) fn snippets_list(&self, text: &str) -> Result<Vec<Snippet>, String> {
        let mut w: Vec<String> = vec!["1".into()];
        let mut p: Vec<Value> = Vec::new();
        let (long, short) = split_terms(parse_terms(text));
        if !long.is_empty() {
            w.push("s.id IN (SELECT rowid FROM snippets_fts WHERE snippets_fts MATCH ?)".into());
            p.push(Value::Text(long.iter().map(|t| fts_quote(t)).collect::<Vec<_>>().join(" AND ")));
        }
        for t in short {
            w.push("(liq_icontains(s.name, ?) OR liq_icontains(s.body, ?))".into());
            p.push(Value::Text(t.clone()));
            p.push(Value::Text(t));
        }
        let sql = format!("SELECT {SNIPPET_COLS} FROM snippets s WHERE {} ORDER BY s.position, s.id", w.join(" AND "));
        let conn = self.lock();
        let mut st = conn.prepare(&sql).map_err(db_err)?;
        let rows = st.query_map(params_from_iter(p.iter()), row_to_snippet).map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// Create (no `id`, appended at the end) or update a snippet. `invalid: name`,
    /// `invalid: body too large`, `invalid: keyword`, `invalid: keyword exists`.
    pub(crate) fn snippet_save(&self, input: &SnippetInput) -> Result<Snippet, String> {
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > SNIPPET_NAME_MAX || name.chars().any(char::is_control) {
            return Err("invalid: name".into());
        }
        if input.body.len() > TEXT_MAX_BYTES {
            return Err("invalid: body too large".into());
        }
        let keyword = input.keyword.as_deref().map(str::trim).filter(|k| !k.is_empty());
        if keyword.is_some_and(|k| k.chars().count() > SNIPPET_KEYWORD_MAX || k.chars().any(char::is_control)) {
            return Err("invalid: keyword".into());
        }
        let now = self.now();
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_err)?;
        if let Some(k) = keyword {
            let clash: Option<i64> = tx
                .query_row(
                    "SELECT id FROM snippets WHERE keyword = ?1 AND id IS NOT ?2",
                    params![k, input.id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_err)?;
            if clash.is_some() {
                return Err("invalid: keyword exists".into());
            }
        }
        let keyword_err = |e: rusqlite::Error| if is_unique_violation(&e) { "invalid: keyword exists".to_string() } else { db_err(e) };
        let id = match input.id {
            Some(id) => {
                let n = tx
                    .execute(
                        "UPDATE snippets SET name = ?1, body = ?2, keyword = ?3, updated_at = ?4 WHERE id = ?5",
                        params![name, input.body, keyword, now, id],
                    )
                    .map_err(keyword_err)?;
                if n == 0 {
                    return Err(format!("not_found: snippet {id}"));
                }
                id
            }
            None => {
                let last: Option<f64> =
                    tx.query_row("SELECT max(position) FROM snippets", [], |r| r.get(0)).map_err(db_err)?;
                tx.execute(
                    "INSERT INTO snippets (name, body, keyword, position, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    params![name, input.body, keyword, last.map_or(1.0, |p| p.floor() + 1.0), now],
                )
                .map_err(keyword_err)?;
                tx.last_insert_rowid()
            }
        };
        let snippet = snippet_by_id(&tx, id)?;
        tx.commit().map_err(db_err)?;
        Ok(snippet)
    }

    pub(crate) fn snippet_delete(&self, id: i64) -> Result<(), String> {
        let n = self.lock().execute("DELETE FROM snippets WHERE id = ?1", [id]).map_err(db_err)?;
        if n == 0 {
            return Err(format!("not_found: snippet {id}"));
        }
        Ok(())
    }

    pub(crate) fn snippet_mark_used(&self, id: i64) -> Result<(), String> {
        let n = self
            .lock()
            .execute("UPDATE snippets SET use_count = use_count + 1 WHERE id = ?1", [id])
            .map_err(db_err)?;
        if n == 0 {
            return Err(format!("not_found: snippet {id}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::TestStore;
    use super::*;

    fn input(id: Option<i64>, name: &str, body: &str, keyword: Option<&str>) -> SnippetInput {
        SnippetInput { id, name: name.into(), body: body.into(), keyword: keyword.map(str::to_string) }
    }

    #[test]
    fn save_list_search_delete() {
        let t = TestStore::new();
        t.set_now(5_000);
        let sig = t.snippet_save(&input(None, " Signature ", "Best regards,\nEddy", Some(";sig"))).unwrap();
        assert_eq!((sig.name.as_str(), sig.keyword.as_deref(), sig.created_at, sig.updated_at), ("Signature", Some(";sig"), 5_000, 5_000));
        let addr = t.snippet_save(&input(None, "Адрес", "г. Казань, ул. Баумана", None)).unwrap();
        assert!(addr.position > sig.position);
        assert_eq!(t.snippets_list("").unwrap().iter().map(|s| s.id).collect::<Vec<_>>(), vec![sig.id, addr.id]);
        assert_eq!(t.snippets_list("regards").unwrap()[0].id, sig.id, "FTS over the body");
        assert_eq!(t.snippets_list("КАЗАН").unwrap()[0].id, addr.id, "Cyrillic, case-insensitive");
        assert_eq!(t.snippets_list("ад").unwrap()[0].id, addr.id, "short term: name/body substring");
        assert!(t.snippets_list("nothing-like-this").unwrap().is_empty());
        // Update.
        t.set_now(6_000);
        let upd = t.snippet_save(&input(Some(sig.id), "Sig", "Cheers", Some(""))).unwrap();
        assert_eq!((upd.name.as_str(), upd.body.as_str(), upd.keyword, upd.updated_at, upd.created_at), ("Sig", "Cheers", None, 6_000, 5_000));
        assert!(t.snippets_list("regards").unwrap().is_empty(), "FTS follows updates");
        // Use counter, delete.
        t.snippet_mark_used(addr.id).unwrap();
        assert_eq!(t.snippets_list("").unwrap()[1].use_count, 1);
        t.snippet_delete(addr.id).unwrap();
        assert_eq!(t.snippets_list("").unwrap().len(), 1);
        assert!(t.snippets_list("казань").unwrap().is_empty());
        assert!(t.snippet_delete(addr.id).unwrap_err().starts_with("not_found:"));
        assert!(t.snippet_mark_used(addr.id).unwrap_err().starts_with("not_found:"));
    }

    #[test]
    fn validation() {
        let t = TestStore::new();
        assert_eq!(t.snippet_save(&input(None, "  ", "b", None)).unwrap_err(), "invalid: name");
        assert_eq!(t.snippet_save(&input(None, &"n".repeat(101), "b", None)).unwrap_err(), "invalid: name");
        assert_eq!(
            t.snippet_save(&input(None, "big", &"b".repeat(TEXT_MAX_BYTES + 1), None)).unwrap_err(),
            "invalid: body too large"
        );
        let a = t.snippet_save(&input(None, "A", "a", Some("kw"))).unwrap();
        assert_eq!(t.snippet_save(&input(None, "B", "b", Some("kw"))).unwrap_err(), "invalid: keyword exists");
        assert_eq!(t.snippet_save(&input(Some(a.id), "A2", "a", Some("kw"))).unwrap().name, "A2", "own keyword is fine");
        assert!(t.snippet_save(&input(Some(999), "X", "x", None)).unwrap_err().starts_with("not_found:"));
        assert_eq!(t.snippet_save(&input(None, "E", "", None)).unwrap().body, "", "an empty body is allowed");
    }
}
