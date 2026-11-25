//! IPC CONTRACT — the shared serde types (docs/ARCHITECTURE.md §9), mirrored 1:1 by
//! `src/lib/ipc/types.ts`. Hub file (coordinator): changes are additive only, in BOTH
//! files, and reported. `tests/contract.test.mjs` checks the two files field by field.
//!
//! Conventions: structs are `camelCase` on the wire; `Option<T>` ↔ `T | null`; an
//! optional TS field (`x?:`) ↔ `Option<T>` with `default` + `skip_serializing_if`, so
//! `None` is omitted rather than sent as `null`. Times are Unix milliseconds (`i64`).
//! Pure data only — defaults/validation live in `settings.rs`, behaviour in modules.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── §9.1 Enumerations ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Link,
    Code,
    Color,
    Contact,
    Image,
    File,
}

#[allow(dead_code)] // W1: used by store/capture
impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Text,
        Kind::Link,
        Kind::Code,
        Kind::Color,
        Kind::Contact,
        Kind::Image,
        Kind::File,
    ];

    /// The wire / SQL spelling (`clips.kind`).
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::Code => "code",
            Kind::Color => "color",
            Kind::Contact => "contact",
            Kind::Image => "image",
            Kind::File => "file",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Text,
    Html,
    Rtf,
    Png,
    Files,
}

#[allow(dead_code)] // W1: used by store/capture
impl Format {
    /// The wire / SQL spelling (`clip_formats.format`).
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Html => "html",
            Format::Rtf => "rtf",
            Format::Png => "png",
            Format::Files => "files",
        }
    }
}

/// `'auto' | 'plain' | { format: Format }`. Serde's externally tagged form gives
/// exactly that: unit variants are bare strings, the newtype variant is
/// `{"format": "<format>"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PasteMode {
    Auto,
    Plain,
    Format(Format),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOrigin {
    Capture,
    User,
    Import,
    Retention,
}

/// `Record<Kind, T>` — one value per kind, always all seven keys on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PerKind<T> {
    pub text: T,
    pub link: T,
    pub code: T,
    pub color: T,
    pub contact: T,
    pub image: T,
    pub file: T,
}

#[allow(dead_code)] // W1: used by store/capture
impl<T> PerKind<T> {
    pub fn get(&self, kind: Kind) -> &T {
        match kind {
            Kind::Text => &self.text,
            Kind::Link => &self.link,
            Kind::Code => &self.code,
            Kind::Color => &self.color,
            Kind::Contact => &self.contact,
            Kind::Image => &self.image,
            Kind::File => &self.file,
        }
    }

    pub fn get_mut(&mut self, kind: Kind) -> &mut T {
        match kind {
            Kind::Text => &mut self.text,
            Kind::Link => &mut self.link,
            Kind::Code => &mut self.code,
            Kind::Color => &mut self.color,
            Kind::Contact => &mut self.contact,
            Kind::Image => &mut self.image,
            Kind::File => &mut self.file,
        }
    }
}

impl<T: Clone> PerKind<T> {
    pub fn splat(v: T) -> Self {
        PerKind {
            text: v.clone(),
            link: v.clone(),
            code: v.clone(),
            color: v.clone(),
            contact: v.clone(),
            image: v.clone(),
            file: v,
        }
    }
}

// ── §9.2 Records ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub id: i64,
    pub exe: String,
    pub name: String,
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipColor {
    pub hex: String,
    pub rgb: String,
    pub hsl: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContactKind {
    Email,
    Phone,
}

/// Stored as JSON in `clips.meta`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ClipColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<ContactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrState {
    None,
    Pending,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub kind: Kind,
    pub title: Option<String>,
    pub preview: String,
    pub app_id: Option<i64>,
    pub created_at: i64,
    pub copied_at: i64,
    pub copy_count: i64,
    pub used_at: Option<i64>,
    pub use_count: i64,
    pub favorite: bool,
    pub folder_id: Option<i64>,
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub char_count: Option<i64>,
    pub file_count: Option<i64>,
    /// `http://liq.localhost/thumb/<id>?v=<n>` for image
    pub thumb_url: Option<String>,
    /// available for "Paste as…"
    pub formats: Vec<Format>,
    pub meta: ClipMeta,
    pub ocr: OcrState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipFile {
    pub path: String,
    pub exists: bool,
    pub size: Option<i64>,
}

/// `ClipDetail extends Clip` — the base fields are flattened into the same object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipDetail {
    #[serde(flatten)]
    pub clip: Clip,
    pub text: Option<String>,
    pub word_count: Option<i64>,
    pub line_count: Option<i64>,
    pub files: Option<Vec<ClipFile>>,
    /// `http://liq.localhost/media/<id>`
    pub image_url: Option<String>,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub position: f64,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub name: String,
    pub body: String,
    pub keyword: Option<String>,
    pub position: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub use_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}

// ── §9.3 Feed queries ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<Kind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorites: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_ids: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedQuery {
    /// free text WITHOUT tokens (the front parses tokens: picker/query.ts)
    pub text: String,
    /// minus-words
    pub exclude: Vec<String>,
    pub filter: Filter,
    /// opaque
    pub cursor: Option<String>,
    /// 40
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub items: Vec<Clip>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub all: i64,
    pub kinds: PerKind<i64>,
    pub favorites: i64,
    pub snippets: i64,
    /// folder id → count (JSON object keys are the ids as strings)
    pub folders: BTreeMap<i64, i64>,
}

// ── §9.4 Settings (defaults + validation: settings.rs) ──────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    System,
    En,
    Ru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClickAction {
    Paste,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RememberSearch {
    #[serde(rename = "never")]
    Never,
    #[serde(rename = "60s")]
    Sec60,
    #[serde(rename = "always")]
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeSeparator {
    Newline,
    Space,
    Tab,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteChord {
    #[serde(rename = "ctrl+v")]
    CtrlV,
    #[serde(rename = "shift+insert")]
    ShiftInsert,
    #[serde(rename = "ctrl+shift+v")]
    CtrlShiftV,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retention {
    #[serde(rename = "1d")]
    Day1,
    #[serde(rename = "1w")]
    Week1,
    #[serde(rename = "1m")]
    Month1,
    #[serde(rename = "3m")]
    Month3,
    #[serde(rename = "6m")]
    Month6,
    #[serde(rename = "1y")]
    Year1,
    #[serde(rename = "forever")]
    Forever,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteKey {
    pub exe: String,
    pub chord: PasteChord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub language: Language,
    pub theme: ThemePref,
    pub autostart: bool,
    pub onboarding_done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerSettings {
    pub position: Position,
    pub center_row: bool,
    /// `"cursor"` | `"primary"` | monitor device name (`MonitorInfo.id`)
    pub monitor: String,
    pub hover_open: bool,
    pub hints: bool,
    pub favorites_first: bool,
    pub click_action: ClickAction,
    pub close_on_copy: bool,
    pub always_plain: bool,
    pub bump_on_paste: bool,
    pub remember_search: RememberSearch,
    pub merge_separator: MergeSeparator,
    pub paste_keys: Vec<PasteKey>,
}

/// `""` = not set. Notation as in Liquid Tools: `"Alt + V"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeySettings {
    pub summon: String,
    pub stack_next: String,
    pub pause_toggle: String,
    pub ignore_next: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSettings {
    pub retention: Retention,
    /// 0 = unlimited
    pub max_entries: i64,
    /// 0 = unlimited
    pub max_size_mb: i64,
    pub types: PerKind<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySettings {
    pub respect_markers: bool,
    pub ignored_apps: Vec<String>,
    pub secret_patterns: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackSettings {
    pub newline_after_paste: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrSettings {
    pub enabled: bool,
    /// `"auto"` | BCP-47 tag of an installed OCR language
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    pub settings_bounds: Option<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Always `1` in v1 (TS literal type `1`); migrations key off it.
    pub version: u32,
    pub general: GeneralSettings,
    pub picker: PickerSettings,
    pub hotkeys: HotkeySettings,
    pub storage: StorageSettings,
    pub privacy: PrivacySettings,
    pub stack: StackSettings,
    pub ocr: OcrSettings,
    pub window: WindowSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsErrorCode {
    Taken,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsError {
    /// dotted path, e.g. `"hotkeys.summon"`
    pub field: String,
    pub code: SettingsErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResult {
    pub settings: Settings,
    pub errors: Vec<SettingsError>,
}

// ── §9.5 Other types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub exe: String,
    pub name: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub elevated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteRequest {
    pub ids: Vec<i64>,
    pub mode: PasteMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_open: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPasteRequest {
    pub text: String,
    pub plain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caret_back: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_open: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PasteResult {
    Pasted,
    Copied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteReason {
    NoTarget,
    Elevated,
    FocusLost,
    ClipboardBusy,
    NotFound,
    StackEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteOutcome {
    pub result: PasteResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PasteReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    /// `None` = recording; `-1` = paused until resumed; otherwise resume time (ms)
    pub paused_until: Option<i64>,
    pub ignore_next: bool,
    pub collect_to_stack: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackItem {
    pub key: String,
    pub clip_id: i64,
    pub kind: Kind,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub items: Vec<StackItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub clips: i64,
    pub db_bytes: i64,
    pub media_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProbe {
    pub found: bool,
    pub path: Option<String>,
    pub clips: i64,
    pub favorites: i64,
    pub folders: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: i64,
    pub skipped: i64,
    pub folders: i64,
    pub errors: i64,
}

// ── Named shapes for the inline types of §9.6 (W0, additive) ────────────────

/// `clip_copy` argument (`{ ids, mode, separator? }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyRequest {
    pub ids: Vec<i64>,
    pub mode: PasteMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
}

/// `{ undoToken }` — clip_set_folder, clips_delete, folder_delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub undo_token: String,
}

/// `{ undoToken: string | null }` — clips_clear (undo only for `'history'`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearResult {
    pub undo_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClearScope {
    History,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DragOutcome {
    Dropped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    Data,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupMode {
    Replace,
    Merge,
}

/// `apps_running` item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    pub exe: String,
    pub name: String,
    pub icon_url: Option<String>,
}

/// `monitors_list` item; `id` (device name) is what goes into `settings.picker.monitor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub primary: bool,
    pub width: i64,
    pub height: i64,
    pub scale: f64,
}

/// `app_info` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMeta {
    pub version: String,
    pub data_dir: String,
    pub log_dir: String,
}

/// `ocr_status` result.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatus {
    pub available: bool,
    pub languages: Vec<String>,
    pub pending: i64,
}

// ── §9.7 Event payloads (W0 names for the inline shapes) ────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipsChange {
    Added,
    Updated,
    Deleted,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipsChanged {
    pub origin: ChangeOrigin,
    pub change: ClipsChange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // W1-D
pub enum PickerMode {
    Summon,
    Hover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W1-D
pub struct PickerOpen {
    pub mode: PickerMode,
    pub target: Option<TargetInfo>,
    pub interactive: bool,
}

/// `{}`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(dead_code)] // W1-D
pub struct PickerClose {}

/// Panel size in logical px (without bleed) + transparent shadow margin `bleed`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W1-D
pub struct PickerLayout {
    pub position: Position,
    pub center_row: bool,
    pub width: f64,
    pub height: f64,
    pub bleed: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W1-D
pub struct SettingsNavigate {
    pub section: String,
}

/// `combo` = recorded chord in `"Alt + V"` notation; `""` = recording cancelled (Esc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W1-D
pub struct HotkeysCaptured {
    pub combo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudKind {
    Copied,
    StackEmpty,
    Paused,
    Elevated,
    /// W1-D (additive): pause toggled off by the hotkey.
    Resumed,
    /// W1-D (additive): "skip next copy" armed by the hotkey.
    SkipNext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W1-D
pub struct HudShow {
    pub kind: HudKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W2-A
pub struct OcrProgress {
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // W2-B
pub struct ImportProgress {
    pub phase: String,
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedTheme {
    Light,
    Dark,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
        expected: serde_json::Value,
    ) {
        let v = serde_json::to_value(value).unwrap();
        assert_eq!(v, expected);
        let back: T = serde_json::from_value(v).unwrap();
        assert_eq!(&back, value);
    }

    #[test]
    fn paste_mode_wire_shape() {
        round_trip(&PasteMode::Auto, json!("auto"));
        round_trip(&PasteMode::Plain, json!("plain"));
        round_trip(&PasteMode::Format(Format::Html), json!({ "format": "html" }));
        assert!(serde_json::from_value::<PasteMode>(json!("html")).is_err());
    }

    #[test]
    fn enums_use_contract_spelling() {
        round_trip(&Kind::Contact, json!("contact"));
        round_trip(&RememberSearch::Sec60, json!("60s"));
        round_trip(&PasteChord::CtrlShiftV, json!("ctrl+shift+v"));
        round_trip(&Retention::Month3, json!("3m"));
        round_trip(&PasteReason::FocusLost, json!("focus_lost"));
        round_trip(&HudKind::StackEmpty, json!("stack_empty"));
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
        }
    }

    #[test]
    fn optional_fields_are_omitted_not_null() {
        round_trip(
            &PasteOutcome { result: PasteResult::Copied, reason: None },
            json!({ "result": "copied" }),
        );
        round_trip(
            &AppInfo { id: 1, exe: "c:\\a.exe".into(), name: "A".into(), icon_url: None, clip_count: Some(3) },
            json!({ "id": 1, "exe": "c:\\a.exe", "name": "A", "iconUrl": null, "clipCount": 3 }),
        );
        round_trip(&ClipMeta::default(), json!({}));
        // A request without the optional keys deserializes.
        let req: PasteRequest = serde_json::from_value(json!({ "ids": [1, 2], "mode": "plain" })).unwrap();
        assert_eq!(req.separator, None);
        assert_eq!(req.keep_open, None);
    }

    #[test]
    fn clip_detail_flattens_clip() {
        let clip = Clip {
            id: 7,
            kind: Kind::Link,
            title: None,
            preview: "example.com".into(),
            app_id: Some(2),
            created_at: 1,
            copied_at: 2,
            copy_count: 1,
            used_at: None,
            use_count: 0,
            favorite: false,
            folder_id: None,
            size: 11,
            width: None,
            height: None,
            char_count: Some(11),
            file_count: None,
            thumb_url: None,
            formats: vec![Format::Text],
            meta: ClipMeta { host: Some("example.com".into()), ..Default::default() },
            ocr: OcrState::None,
        };
        let detail = ClipDetail {
            clip,
            text: Some("example.com".into()),
            word_count: Some(1),
            line_count: Some(1),
            files: None,
            image_url: None,
            ocr_text: None,
        };
        let v = serde_json::to_value(&detail).unwrap();
        assert_eq!(v["id"], json!(7));
        assert_eq!(v["kind"], json!("link"));
        assert_eq!(v["meta"], json!({ "host": "example.com" }));
        assert_eq!(v["wordCount"], json!(1));
        assert_eq!(v["files"], json!(null));
        let back: ClipDetail = serde_json::from_value(v).unwrap();
        assert_eq!(back, detail);
    }

    #[test]
    fn counts_folder_keys_are_strings() {
        let mut folders = BTreeMap::new();
        folders.insert(3, 10);
        let counts = Counts { all: 10, kinds: PerKind::splat(1), favorites: 0, snippets: 2, folders };
        let v = serde_json::to_value(&counts).unwrap();
        assert_eq!(v["folders"], json!({ "3": 10 }));
        assert_eq!(v["kinds"]["image"], json!(1));
        let back: Counts = serde_json::from_value(v).unwrap();
        assert_eq!(back, counts);
    }

    #[test]
    fn picker_close_is_empty_object() {
        round_trip(&PickerClose {}, json!({}));
    }
}
