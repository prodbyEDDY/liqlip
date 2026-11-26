//! Raw-body framing of `shot_commit` / `shot_editor_commit` (ARCHITECTURE §18.4; W5-A, hub —
//! changes go through the coordinator):
//!
//! ```text
//! [u32 LE: N] [N bytes UTF-8 JSON: ShotCommit | ShotEditorCommit] [RGBA: width*height*4 bytes, or nothing]
//! ```
//!
//! The webview sends the body with `invoke(cmd, bytes)` (`@tauri-apps/api` 2.11 `InvokeArgs`
//! accepts a `Uint8Array`), so the pixels never become JSON and nothing rides in headers (no
//! header size limit applies). Tauri hands the command the body as `InvokeBody::Raw` (tauri
//! 2.11.2 `ipc/mod.rs`: `Request::body() -> &InvokeBody`). If the custom-protocol IPC failed
//! and Tauri fell back to `postMessage`, the same bytes arrive as a JSON array of numbers
//! (`scripts/process-ipc-message-fn.js` serializes a `Uint8Array` with `Array.from`) — accepted
//! too, at the cost of one copy.

use crate::model::{ShotCommit, ShotEditorCommit};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::borrow::Cow;
use tauri::ipc::InvokeBody;

/// Upper bound of the JSON header. An annotation document can carry pasted pictures as
/// `data:` URLs (`AnnotObject` `image`), so it may be large — but not unbounded.
pub(crate) const MAX_HEADER_BYTES: usize = 64 * 1024 * 1024;
/// Upper bound of either side of the pixel block (a sanity check before multiplying).
pub(crate) const MAX_SIDE: u32 = 32_768;

/// A header that says how many pixels follow it (`width`/`height` of the RGBA block; both 0
/// when the body carries no pixels).
pub(crate) trait FrameHeader {
    fn dims(&self) -> (u32, u32);
}

impl FrameHeader for ShotCommit {
    fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl FrameHeader for ShotEditorCommit {
    fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// The bytes of a raw-body invoke (borrowed for `InvokeBody::Raw`).
pub(crate) fn bytes(body: &InvokeBody) -> Result<Cow<'_, [u8]>, String> {
    match body {
        InvokeBody::Raw(b) => Ok(Cow::Borrowed(b.as_slice())),
        // postMessage fallback: the Uint8Array arrives as `[n, n, …]`.
        InvokeBody::Json(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_u64().filter(|n| *n <= 255).map(|n| n as u8))
            .collect::<Option<Vec<u8>>>()
            .map(Cow::Owned)
            .ok_or_else(|| "invalid: body is not a byte array".to_string()),
        InvokeBody::Json(_) => Err("invalid: expected a raw body (frameBody)".into()),
    }
}

/// Split a framed body into its JSON header and its pixels (`None` — the body has none).
/// Every malformed body is `invalid: …`; the pixel block must be exactly `width*height*4`.
pub(crate) fn parse<T: DeserializeOwned + FrameHeader>(bytes: &[u8]) -> Result<(T, Option<&[u8]>), String> {
    let len_bytes: [u8; 4] = bytes
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| "invalid: body shorter than its length prefix".to_string())?;
    let n = u32::from_le_bytes(len_bytes) as usize;
    if n == 0 || n > MAX_HEADER_BYTES {
        return Err(format!("invalid: header length {n}"));
    }
    let json = bytes
        .get(4..4 + n)
        .ok_or_else(|| format!("invalid: header of {n} bytes is cut short"))?;
    let head: T = serde_json::from_slice(json).map_err(|e| format!("invalid: header: {e}"))?;
    let rest = &bytes[4 + n..];
    let rgba = match head.dims() {
        (0, 0) if rest.is_empty() => None,
        (0, 0) => return Err(format!("invalid: {} pixel bytes for a 0×0 picture", rest.len())),
        (w, h) if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE => {
            return Err(format!("invalid: picture size {w}×{h}"));
        }
        (w, h) => {
            let want = u64::from(w) * u64::from(h) * 4;
            if rest.len() as u64 != want {
                return Err(format!("invalid: {w}×{h} needs {want} pixel bytes, got {}", rest.len()));
            }
            Some(rest)
        }
    };
    Ok((head, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Rect, ShotAction, ShotEditorAction};
    use serde_json::json;

    /// What `frameBody(json, rgba)` (src/lib/ipc/frame.ts) builds.
    fn frame(head: &Value, rgba: &[u8]) -> Vec<u8> {
        let json = serde_json::to_vec(head).unwrap();
        let mut out = (json.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&json);
        out.extend_from_slice(rgba);
        out
    }

    fn commit(w: u32, h: u32) -> Value {
        json!({ "sessionId": 3, "rect": { "x": 10.5, "y": 20, "w": 2, "h": 1 }, "action": "save_as", "doc": null, "width": w, "height": h })
    }

    #[test]
    fn a_commit_without_pixels() {
        let body = frame(&commit(0, 0), &[]);
        let (head, rgba) = parse::<ShotCommit>(&body).unwrap();
        assert_eq!(head.session_id, 3);
        assert_eq!(head.action, ShotAction::SaveAs);
        assert_eq!(head.rect, Rect { x: 10.5, y: 20.0, w: 2.0, h: 1.0 });
        assert_eq!(head.doc, None, "doc: null");
        assert!(rgba.is_none());
    }

    #[test]
    fn a_commit_with_pixels_and_a_document() {
        let mut head = commit(2, 1);
        head["doc"] = json!({ "v": 1, "w": 2, "h": 1, "unit": 1.5, "crop": null, "objects": [{ "id": "a", "kind": "line" }] });
        let px = [1, 2, 3, 4, 5, 6, 7, 8];
        let body = frame(&head, &px);
        let (head, rgba) = parse::<ShotCommit>(&body).unwrap();
        assert_eq!(rgba, Some(&px[..]));
        assert_eq!(head.doc.as_ref().unwrap()["objects"][0]["kind"], json!("line"), "the document is kept opaque");
    }

    #[test]
    fn an_editor_commit() {
        let head = json!({ "clipId": 9, "action": "pin", "doc": { "v": 1, "objects": [] }, "width": 1, "height": 1 });
        let body = frame(&head, &[0, 0, 0, 255]);
        let (head, rgba) = parse::<ShotEditorCommit>(&body).unwrap();
        assert_eq!((head.clip_id, head.action), (9, ShotEditorAction::Pin));
        assert_eq!(rgba.map(<[u8]>::len), Some(4));
    }

    #[test]
    fn broken_bodies_are_invalid() {
        let bad = |body: &[u8]| {
            let e = parse::<ShotCommit>(body).map(|_| ()).unwrap_err();
            assert!(e.starts_with("invalid: "), "{e}");
        };
        bad(&[]);
        bad(&[5, 0, 0]);
        bad(&[0, 0, 0, 0]); // empty header
        bad(&[255, 255, 255, 255, b'{']); // header longer than the body
        let mut cut = frame(&commit(0, 0), &[]);
        cut.truncate(cut.len() - 1);
        bad(&cut); // JSON cut short
        bad(&frame(&json!({ "sessionId": 3 }), &[])); // fields missing
        bad(&frame(&json!([1, 2]), &[])); // not an object
        bad(&frame(&commit(0, 0), &[1, 2, 3, 4])); // pixels for a 0×0 picture
        bad(&frame(&commit(2, 0), &[])); // one side 0
        bad(&frame(&commit(2, 1), &[1, 2, 3, 4])); // too few bytes
        bad(&frame(&commit(1, 1), &[1, 2, 3, 4, 5])); // too many
        bad(&frame(&commit(MAX_SIDE + 1, 1), &[])); // absurd size
        let mut wrong = commit(0, 0);
        wrong["action"] = json!("explode");
        bad(&frame(&wrong, &[]));
    }

    #[test]
    fn body_bytes_raw_and_the_post_message_fallback() {
        let raw = InvokeBody::Raw(vec![1, 2, 3]);
        let b = bytes(&raw).unwrap();
        assert!(matches!(b, Cow::Borrowed(_)), "a raw body is not copied");
        assert_eq!(b.as_ref(), &[1, 2, 3]);
        let arr = InvokeBody::Json(json!([4, 0, 255]));
        assert_eq!(bytes(&arr).unwrap().as_ref(), &[4, 0, 255]);
        assert!(bytes(&InvokeBody::Json(json!([256]))).unwrap_err().starts_with("invalid: "));
        assert!(bytes(&InvokeBody::Json(json!({ "sessionId": 1 }))).unwrap_err().starts_with("invalid: "));
    }
}
