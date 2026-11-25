//! Native OLE drag-out of records (SPEC §3.6) — owner **W1-C**. Ported from Liquid Tools
//! `src-tauri/src/dragout.rs` (read-only source), without its `screenshot::wait_file_ready`
//! dependency (the files are already complete on disk) and with text drags added.
//!
//! What a drag offers (a target takes the first format it understands):
//! - **text** records → `CF_UNICODETEXT` (+ `CF_TEXT` in the ANSI code page);
//! - **files** → the shell's own data object for the paths (CF_HDROP, shell ID lists…) +
//!   the paths as text + `Preferred DropEffect = COPY`;
//! - **one image** → its `media\<id>.png` as a file (CF_HDROP) + the virtual-file pair
//!   `FileGroupDescriptorW`/`FileContents` (what Chromium/Electron targets ingest on drop) +
//!   `PNG` + `CF_DIB`, with the image's thumbnail as the drag image
//!   (`IDragSourceHelper::InitializeFromBitmap`).
//!
//! The drag runs on the MAIN thread (OLE drag sources need an STA with a message loop);
//! `SHDoDragDrop` supplies a generic drag image when none was set ("the Shell provides a
//! generic drag image" — learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shdodragdrop).

use super::compose::file_paths;
use crate::model::{DragOutcome, Kind};
use crate::store::PastePayload;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// `true` while an OLE drag started by Liqlip is in progress — the shelf's focus-loss exit
/// (W1-D) must not close the shelf during it.
#[allow(dead_code)] // W1-D reads it in its Focused(false) handler
pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// What is dragged (before disk checks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragContent {
    Text(String),
    /// Local files; `image` = the single dragged record is an image (its PNG is `paths[0]`).
    Paths { paths: Vec<String>, image: bool },
}

/// Records → drag content. Files and images (as their PNG) win over text; text-only
/// selections drag as text joined by `sep`.
pub fn drag_content(payloads: &[PastePayload], media: impl Fn(&std::path::Path) -> PathBuf, sep: &str) -> Result<DragContent, String> {
    let mut paths: Vec<String> = Vec::new();
    for p in payloads {
        match p.kind {
            Kind::File => paths.extend(file_paths(p)),
            Kind::Image => {
                if let Some(png) = &p.png_path {
                    paths.push(media(png).to_string_lossy().into_owned());
                }
            }
            _ => {}
        }
    }
    let mut seen = std::collections::HashSet::new();
    paths.retain(|x| seen.insert(x.to_lowercase()));
    if !paths.is_empty() {
        let image = payloads.len() == 1 && payloads[0].kind == Kind::Image;
        return Ok(DragContent::Paths { paths, image });
    }
    let texts: Vec<&str> = payloads.iter().filter(|p| !matches!(p.kind, Kind::File | Kind::Image)).filter_map(|p| p.text.as_deref()).collect();
    if texts.is_empty() {
        return Err("invalid: nothing to drag".into());
    }
    Ok(DragContent::Text(texts.join(sep)))
}

/// Start the drag and resolve after the drop / cancel. `prepare` work (disk reads, image
/// decode) happens before, on the caller's blocking thread.
pub async fn drag_out(app: tauri::AppHandle, content: DragContent) -> Result<DragOutcome, String> {
    #[cfg(windows)]
    {
        let prepared = tauri::async_runtime::spawn_blocking(move || win::prepare(content))
            .await
            .map_err(|e| format!("failed: {e}"))??;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            ACTIVE.store(true, Ordering::SeqCst);
            let r = win::do_drag(prepared);
            ACTIVE.store(false, Ordering::SeqCst);
            let _ = tx.send(r);
        })
        .map_err(|e| format!("failed: {e}"))?;
        rx.await.map_err(|e| format!("failed: {e}"))?
    }
    #[cfg(not(windows))]
    {
        let _ = (app, content);
        Err("failed: drag-out is Windows-only".into())
    }
}

#[cfg(windows)]
mod win {
    use super::DragContent;
    use crate::model::DragOutcome;
    use crate::paste::writer::{dib_from_rgba, DROPEFFECT_COPY as COPY};
    use std::iter::once;
    use windows::core::{implement, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, DATA_S_SAMEFORMATETC, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_OUTOFMEMORY, FILETIME,
        HGLOBAL, POINT, POINTL, SIZE, S_OK,
    };
    use windows::Win32::Globalization::{WideCharToMultiByte, CP_ACP};
    use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, HGDIOBJ};
    use windows::Win32::System::Com::{
        CoCreateInstance, IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, CLSCTX_ALL, DVASPECT_CONTENT,
        FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
    };
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{
        IDropSource, IDropSource_Impl, OleInitialize, OleUninitialize, CF_DIB, CF_TEXT, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY,
    };
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, IDragSourceHelper, ILCreateFromPathW, ILFree, SHCreateDataObject, SHCreateShellItemArrayFromIDLists,
        SHCreateStdEnumFmtEtc, SHDoDragDrop, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_PREFERREDDROPEFFECT,
        CLSID_DragDropHelper, FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI, FD_UNICODE, FILEDESCRIPTORW, FILEGROUPDESCRIPTORW,
        SHDRAGIMAGE,
    };

    const CF_DIB_ID: u16 = CF_DIB.0;
    const CF_TEXT_ID: u16 = CF_TEXT.0;
    const CF_UNICODETEXT_ID: u16 = CF_UNICODETEXT.0;
    /// Long side of the drag preview.
    const DRAG_IMAGE_MAX: u32 = 192;

    /// Single-image payload, fully precomputed so `GetData` is only an HGLOBAL copy.
    pub struct ImagePayload {
        file: Vec<u8>,
        dib: Vec<u8>,
        descriptor: Vec<u8>,
        /// Premultiplied BGRA preview + size (drag image).
        preview: (Vec<u8>, i32, i32),
    }

    /// Everything the main thread needs.
    pub struct Prepared {
        paths: Vec<String>,
        text: Vec<u16>,
        text_ansi: Vec<u8>,
        image: Option<ImagePayload>,
    }

    fn ansi(wide_z: &[u16]) -> Vec<u8> {
        // SAFETY: size query then a buffer of that size (WideCharToMultiByte, CP_ACP);
        // learn.microsoft.com/windows/win32/api/stringapiset/nf-stringapiset-widechartomultibyte
        unsafe {
            let n = WideCharToMultiByte(CP_ACP, 0, wide_z, None, PCSTR_NULL, None);
            if n <= 0 {
                return vec![0];
            }
            let mut out = vec![0u8; n as usize];
            WideCharToMultiByte(CP_ACP, 0, wide_z, Some(&mut out), PCSTR_NULL, None);
            out
        }
    }
    const PCSTR_NULL: windows::core::PCSTR = windows::core::PCSTR::null();

    fn file_group_descriptor(name: &str, size: u64) -> Vec<u8> {
        let mut cfn = [0u16; 260];
        for (i, c) in name.encode_utf16().take(259).enumerate() {
            cfn[i] = c;
        }
        let fd = FILEDESCRIPTORW {
            dwFlags: (FD_UNICODE.0 | FD_FILESIZE.0 | FD_ATTRIBUTES.0 | FD_PROGRESSUI.0) as u32,
            clsid: windows::core::GUID::from_u128(0),
            sizel: SIZE::default(),
            pointl: POINTL::default(),
            dwFileAttributes: 0x80, // FILE_ATTRIBUTE_NORMAL
            ftCreationTime: FILETIME::default(),
            ftLastAccessTime: FILETIME::default(),
            ftLastWriteTime: FILETIME::default(),
            nFileSizeHigh: (size >> 32) as u32,
            nFileSizeLow: size as u32,
            cFileName: cfn,
        };
        let fgd = FILEGROUPDESCRIPTORW { cItems: 1, fgd: [fd] };
        // SAFETY: a plain-old-data struct viewed as its bytes.
        unsafe {
            std::slice::from_raw_parts(&fgd as *const _ as *const u8, std::mem::size_of::<FILEGROUPDESCRIPTORW>()).to_vec()
        }
    }

    fn image_payload(path: &str) -> Option<ImagePayload> {
        let file = std::fs::read(path).ok()?;
        let img = image::load_from_memory(&file).ok()?.to_rgba8(); // decoded once
        let (w, h) = (img.width(), img.height());
        if w == 0 || h == 0 {
            return None;
        }
        let dib = dib_from_rgba(w, h, img.as_raw());
        let (tw, th) = if w.max(h) > DRAG_IMAGE_MAX {
            let s = DRAG_IMAGE_MAX as f64 / w.max(h) as f64;
            (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
        } else {
            (w, h)
        };
        let thumb = image::imageops::thumbnail(&img, tw, th);
        let mut bgra = thumb.into_raw();
        for px in bgra.chunks_exact_mut(4) {
            let a = px[3] as u32;
            let (r, g, b) = ((px[0] as u32 * a / 255) as u8, (px[1] as u32 * a / 255) as u8, (px[2] as u32 * a / 255) as u8);
            px[0] = b;
            px[1] = g;
            px[2] = r;
        }
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image.png".into());
        let descriptor = file_group_descriptor(&name, file.len() as u64);
        Some(ImagePayload { file, dib, descriptor, preview: (bgra, tw as i32, th as i32) })
    }

    /// Disk work off the UI thread: existence checks, the image decode.
    pub fn prepare(content: DragContent) -> Result<Prepared, String> {
        match content {
            DragContent::Text(text) => {
                let wide: Vec<u16> = text.encode_utf16().chain(once(0)).collect();
                let text_ansi = ansi(&wide);
                Ok(Prepared { paths: Vec::new(), text: wide, text_ansi, image: None })
            }
            DragContent::Paths { paths, image } => {
                let paths: Vec<String> = paths.into_iter().filter(|p| std::path::Path::new(p).exists()).collect();
                if paths.is_empty() {
                    return Err("not_found: the files no longer exist".into());
                }
                let joined = paths.join("\r\n");
                let wide: Vec<u16> = joined.encode_utf16().chain(once(0)).collect();
                let text_ansi = ansi(&wide);
                let image = if image { image_payload(&paths[0]) } else { None };
                Ok(Prepared { paths, text: wide, text_ansi, image })
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Formats {
        fd: u16,
        fc: u16,
        png: u16,
        pde: u16,
    }

    /// Wraps the shell data object (CF_HDROP, the drag helper's private formats) and adds the
    /// virtual-file pair, CF_DIB, PNG, text and Preferred DropEffect on top.
    #[implement(IDataObject)]
    struct DragDataObject {
        inner: IDataObject,
        formats: Formats,
        image: Option<ImagePayload>,
        text: Vec<u16>,
        text_ansi: Vec<u8>,
    }

    fn bytes_of_u16(v: &[u16]) -> &[u8] {
        // SAFETY: u16 → its little-endian bytes (Windows is LE), same lifetime.
        unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) }
    }

    /// A GMEM_MOVEABLE HGLOBAL with `data` (the drop target frees it via ReleaseStgMedium).
    fn hglobal_from_bytes(data: &[u8]) -> windows::core::Result<HGLOBAL> {
        // SAFETY: allocation sized for `data`, locked only for the copy.
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1))?;
            let ptr = GlobalLock(h);
            if ptr.is_null() {
                let _ = windows::Win32::Foundation::GlobalFree(Some(h));
                return Err(windows::core::Error::from_hresult(E_OUTOFMEMORY));
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
            let _ = GlobalUnlock(h);
            Ok(h)
        }
    }

    impl DragDataObject {
        fn own_hglobal(&self, f: &FORMATETC) -> Option<windows::core::Result<HGLOBAL>> {
            if f.dwAspect != DVASPECT_CONTENT.0 || (f.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
                return None;
            }
            let cf = f.cfFormat;
            let img = self.image.as_ref();
            if cf == CF_UNICODETEXT_ID {
                return Some(hglobal_from_bytes(bytes_of_u16(&self.text)));
            }
            if cf == CF_TEXT_ID {
                return Some(hglobal_from_bytes(&self.text_ansi));
            }
            if cf == self.formats.pde {
                return Some(hglobal_from_bytes(&COPY.to_le_bytes()));
            }
            if cf == self.formats.fd {
                return img.map(|i| hglobal_from_bytes(&i.descriptor));
            }
            if cf == self.formats.fc && f.lindex <= 0 {
                return img.map(|i| hglobal_from_bytes(&i.file));
            }
            if cf == CF_DIB_ID {
                return img.map(|i| hglobal_from_bytes(&i.dib));
            }
            if cf == self.formats.png {
                return img.map(|i| hglobal_from_bytes(&i.file));
            }
            None
        }

        fn mine(&self, f: &FORMATETC) -> bool {
            if f.dwAspect != DVASPECT_CONTENT.0 || (f.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
                return false;
            }
            let cf = f.cfFormat;
            cf == CF_UNICODETEXT_ID
                || cf == CF_TEXT_ID
                || cf == self.formats.pde
                || (self.image.is_some()
                    && (cf == self.formats.fd || (cf == self.formats.fc && f.lindex <= 0) || cf == CF_DIB_ID || cf == self.formats.png))
        }

        fn offered_formats(&self) -> Vec<FORMATETC> {
            let mk = |cf: u16, lindex: i32| FORMATETC {
                cfFormat: cf,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            let mut out = Vec::new();
            if self.image.is_some() {
                out.push(mk(self.formats.fd, -1));
                out.push(mk(self.formats.fc, 0));
                out.push(mk(self.formats.png, -1));
                out.push(mk(CF_DIB_ID, -1));
            }
            // SAFETY: enumerating the inner (shell) object's GET formats.
            unsafe {
                if let Ok(e) = self.inner.EnumFormatEtc(1 /* DATADIR_GET */) {
                    let mut buf = [FORMATETC::default(); 16];
                    loop {
                        let mut fetched = 0u32;
                        let hr = e.Next(&mut buf, Some(&mut fetched as *mut u32));
                        out.extend(buf.iter().take(fetched as usize).copied());
                        if hr != S_OK || fetched == 0 {
                            break;
                        }
                    }
                }
            }
            out.push(mk(CF_UNICODETEXT_ID, -1));
            out.push(mk(CF_TEXT_ID, -1));
            out.push(mk(self.formats.pde, -1));
            out
        }
    }

    #[allow(non_snake_case)]
    impl IDataObject_Impl for DragDataObject_Impl {
        fn GetData(&self, pformatetcin: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            // SAFETY: COM contract — a valid FORMATETC pointer or null.
            if let Some(f) = unsafe { pformatetcin.as_ref() } {
                if let Some(h) = self.own_hglobal(f) {
                    return Ok(STGMEDIUM {
                        tymed: TYMED_HGLOBAL.0 as u32,
                        u: STGMEDIUM_0 { hGlobal: h? },
                        pUnkForRelease: std::mem::ManuallyDrop::new(None),
                    });
                }
            }
            unsafe { self.inner.GetData(pformatetcin) }
        }

        fn GetDataHere(&self, pformatetc: *const FORMATETC, pmedium: *mut STGMEDIUM) -> windows::core::Result<()> {
            unsafe { self.inner.GetDataHere(pformatetc, pmedium) }
        }

        fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
            if let Some(f) = unsafe { pformatetc.as_ref() } {
                if self.mine(f) {
                    return S_OK;
                }
            }
            unsafe { self.inner.QueryGetData(pformatetc) }
        }

        fn GetCanonicalFormatEtc(&self, _in: *const FORMATETC, pformatetcout: *mut FORMATETC) -> HRESULT {
            if let Some(out) = unsafe { pformatetcout.as_mut() } {
                out.ptd = std::ptr::null_mut();
            }
            DATA_S_SAMEFORMATETC
        }

        fn SetData(&self, pformatetc: *const FORMATETC, pmedium: *const STGMEDIUM, frelease: windows::core::BOOL) -> windows::core::Result<()> {
            // The drag-image helper stores its private formats here.
            unsafe { self.inner.SetData(pformatetc, pmedium, frelease.as_bool()) }
        }

        fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
            if dwdirection != 1 {
                return unsafe { self.inner.EnumFormatEtc(dwdirection) };
            }
            unsafe { SHCreateStdEnumFmtEtc(&self.offered_formats()) }
        }

        fn DAdvise(&self, pformatetc: *const FORMATETC, advf: u32, padvsink: windows::core::Ref<IAdviseSink>) -> windows::core::Result<u32> {
            unsafe { self.inner.DAdvise(pformatetc, advf, padvsink.as_ref()) }
        }

        fn DUnadvise(&self, dwconnection: u32) -> windows::core::Result<()> {
            unsafe { self.inner.DUnadvise(dwconnection) }
        }

        fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
            unsafe { self.inner.EnumDAdvise() }
        }
    }

    /// Esc cancels, releasing the left button drops.
    #[implement(IDropSource)]
    struct DropSource;

    #[allow(non_snake_case)]
    impl IDropSource_Impl for DropSource_Impl {
        fn QueryContinueDrag(&self, fescapepressed: windows::core::BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
            if fescapepressed.as_bool() {
                DRAGDROP_S_CANCEL
            } else if (grfkeystate & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
                DRAGDROP_S_DROP
            } else {
                S_OK
            }
        }

        fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }

    fn register(name: PCWSTR) -> u16 {
        // SAFETY: a static NUL-terminated format name.
        unsafe { RegisterClipboardFormatW(name) as u16 }
    }

    /// The shell data object for files (CF_HDROP + shell formats), or an empty one for text.
    unsafe fn inner_object(paths: &[String]) -> Result<IDataObject, String> {
        unsafe {
            if paths.is_empty() {
                return SHCreateDataObject::<_, IDataObject>(None, None, None::<&IDataObject>).map_err(|e| format!("failed: {e}"));
            }
            let mut pidls: Vec<*const ITEMIDLIST> = Vec::with_capacity(paths.len());
            for p in paths {
                let wide: Vec<u16> = p.encode_utf16().chain(once(0)).collect();
                let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
                if pidl.is_null() {
                    for &q in &pidls {
                        ILFree(Some(q));
                    }
                    return Err("not_found: a dragged file no longer exists".into());
                }
                pidls.push(pidl.cast_const());
            }
            let array = SHCreateShellItemArrayFromIDLists(&pidls);
            for &q in &pidls {
                ILFree(Some(q));
            }
            let array = array.map_err(|e| format!("failed: {e}"))?;
            array.BindToHandler(None, &BHID_DataObject).map_err(|e| format!("failed: {e}"))
        }
    }

    /// The modal OLE drag. MAIN thread only.
    pub fn do_drag(prepared: Prepared) -> Result<DragOutcome, String> {
        // SAFETY: OLE initialized (and balanced) on the main thread; every COM object is
        // released before OleUninitialize (declared after the guard → dropped first).
        unsafe {
            OleInitialize(None).map_err(|e| format!("failed: OleInitialize {e}"))?;
            struct OleGuard;
            impl Drop for OleGuard {
                fn drop(&mut self) {
                    unsafe { OleUninitialize() };
                }
            }
            let _ole = OleGuard;

            let inner = inner_object(&prepared.paths)?;
            let formats = Formats {
                fd: register(CFSTR_FILEDESCRIPTORW),
                fc: register(CFSTR_FILECONTENTS),
                png: register(windows::core::w!("PNG")),
                pde: register(CFSTR_PREFERREDDROPEFFECT),
            };
            let preview = prepared.image.as_ref().map(|i| i.preview.clone());
            let data: IDataObject =
                DragDataObject { inner, formats, image: prepared.image, text: prepared.text, text_ansi: prepared.text_ansi }.into();

            // Drag image: the image's thumbnail. InitializeFromBitmap owns the HBITMAP on
            // success; free it ourselves otherwise.
            if let Some((bgra, w, h)) = preview {
                let hbm = CreateBitmap(w, h, 1, 32, Some(bgra.as_ptr() as *const core::ffi::c_void));
                if !hbm.is_invalid() {
                    let shdi = SHDRAGIMAGE {
                        sizeDragImage: SIZE { cx: w, cy: h },
                        ptOffset: POINT { x: w / 2, y: h / 2 },
                        hbmpDragImage: hbm,
                        crColorKey: COLORREF(0xFFFF_FFFF),
                    };
                    let ok = CoCreateInstance::<_, IDragSourceHelper>(&CLSID_DragDropHelper, None, CLSCTX_ALL)
                        .and_then(|helper| helper.InitializeFromBitmap(&shdi, &data))
                        .is_ok();
                    if !ok {
                        let _ = DeleteObject(HGDIOBJ(hbm.0));
                    }
                }
            }

            let source: IDropSource = DropSource.into();
            match SHDoDragDrop(None, &data, &source, DROPEFFECT_COPY) {
                // pdwEffect "is set only if the operation is not canceled": NONE = cancelled
                // or rejected by the target.
                Ok(effect) if (effect.0 & DROPEFFECT_COPY.0) != 0 => Ok(DragOutcome::Dropped),
                Ok(_) => Ok(DragOutcome::Cancelled),
                Err(e) => Err(format!("failed: drag {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::compose::tests::{files, image, text};
    use std::path::Path;

    fn ident(p: &Path) -> PathBuf {
        p.to_path_buf()
    }

    #[test]
    fn text_only_drags_as_text() {
        assert_eq!(drag_content(&[text(1, "a"), text(2, "b")], ident, "\r\n"), Ok(DragContent::Text("a\r\nb".into())));
    }

    #[test]
    fn single_image_is_its_png_file() {
        assert_eq!(
            drag_content(&[image(7)], ident, "\r\n"),
            Ok(DragContent::Paths { paths: vec!["C:\\data\\media\\7.png".into()], image: true })
        );
    }

    #[test]
    fn files_and_images_win_over_text_and_dedupe() {
        let c = drag_content(&[text(1, "t"), files(2, &["C:\\a", "C:\\b"]), files(3, &["c:\\A"]), image(4)], ident, "\r\n").unwrap();
        assert_eq!(
            c,
            DragContent::Paths { paths: vec!["C:\\a".into(), "C:\\b".into(), "C:\\data\\media\\4.png".into()], image: false }
        );
        assert!(drag_content(&[], ident, "").is_err());
    }
}
