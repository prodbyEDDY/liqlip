# Liqlip — архитектура и IPC-контракт

> Технический дизайн v1. Что строим — [SPEC.md](SPEC.md); статус — [ROADMAP.md](ROADMAP.md).
> Это **контракт** для параллельных агентов: типы, команды, события, владение файлами.
> Менять контракт может только координатор (агенты — аддитивно и с пометкой в отчёте, см. §13).
> Замер: 2026-11-25 (проект, кода ещё нет).

## 0. Стек

| Слой | Выбор | Примечание |
|---|---|---|
| Оболочка | **Tauri 2** (как Liquid Tools, 2.11.x) | `tray-icon`; плагины: `single-instance`, `autostart`, `dialog` (выбор exe, экспорт/импорт), `opener` (открыть ссылку/файл) |
| UI | **Svelte 5 (runes)** + **Vite MPA** + **Tailwind v4** + TypeScript | одна HTML-точка на окно, как в Liquid Tools |
| Шрифты | `@fontsource-variable/inter`, `@fontsource-variable/jetbrains-mono` | self-hosted |
| Иконки | Heroicons outline через локальный `src/lib/heroicons/` + `Ico` (как Liquid Tools после 2026-09-22) | |
| Rust | `rusqlite` (bundled; версию определяет W0/W1-A — в Liquid Tools закреплена `=0.38` из-за rustc 1.94, сейчас стоит rustc **1.99** — проверить, собирается ли новее), `windows` 0.62 (Win32/WinRT), `arboard` 3.6 (только если нужен — запись/чтение делаем своими Win32-вызовами), `image` 0.25 (`png`), `serde`, `serde_json`, `parking_lot`, `base64`, `tokio` (`sync`) | `enigo` **не** используем — `SendInput` напрямую |
| Тесты | `node:test` + `esbuild` (как Liquid Tools) для чистой TS-логики; `cargo test` для Rust — **если заработает** в новом крейте (в Liquid Tools тест-бинарники падают с `STATUS_ENTRYPOINT_NOT_FOUND`, DOC-08; W0 проверяет) | |

Toolchain на машине (2026-11-25): rustc/cargo 1.99, node 24.21, pnpm 12.6.

Идентификатор `com.liqlip.app`, productName `Liqlip`. Данные: `%APPDATA%\com.liqlip.app\`.

## 1. Раскладка репозитория

```
liqlip/
  picker.html · settings.html · hud.html          одна точка входа на окно (Vite MPA)
  src/
    app.css                     токены (перенос из Liquid Tools, монохром-лок)
    lib/
      components/               общие компоненты (перенос: Ico, Button, Toggle, SegmentedControl, Dropdown,
                                Menu/MenuItem, Skeleton, EmptyState, Toast/FlashPill, Kbd, Badge, BigTypeList …)
      heroicons/ · icons.ts     иконки
      i18n/                     en.ts (источник ключей) · ru.ts (Record<TranslationKey,string>) · index.svelte.ts
      ipc/
        types.ts                ← КОНТРАКТ (§9), зеркало src-tauri/src/model.rs
        commands.ts             типизированные обёртки invoke (одна функция на команду)
        events.ts               типизированные подписки
        mock/                   браузерный адаптер: без Tauri команды обслуживает in-memory сид
      motion/ · theme.svelte.ts · flash.svelte.ts · hotkeyCapture.ts
    windows/
      picker/                   шелф (Picker.svelte + компоненты, cards/, layout.ts, query.ts, transforms.ts …)
      settings/                 окно настроек + онбординг
      hud/                      HUD-тост
  src-tauri/
    src/
      lib.rs                    ← ФАЙЛ-УЗЕЛ: run(), плагины, окна, порядок init, регистрация команд, URI-схема
      model.rs                  ← КОНТРАКТ: общие serde-типы (§9)
      settings.rs               загрузка/сохранение/валидация Settings + рассылка изменений
      store/                    SQLite: схема, фид, поиск, папки, избранное, сниппеты, ретенция, медиа, undo
      capture/                  монитор буфера: listener, снимок форматов, приватность, классификация, владелец, иконки
      paste/                    запись в буфер, фокус, SendInput, UIPI, drag-out, очередь вставки
      picker/                   окно шелфа: докинг, вызов, наведение, выходы; HUD
      hotkeys/                  движок глобальных хоткеев + запись сочетания
      ocr/                      (W2) Windows.Media.Ocr, фоновая очередь
      import/                   (W2) импорт из Liquid Tools, резервные копии
      tray.rs · theme.rs · logx.rs · fsx.rs · autostart.rs · i18n_native.rs
    tauri.conf.json · capabilities/default.json
  tests/                        *.test.mjs (node:test)
  docs/                         эта библиотека
```

## 2. Окна

Все три окна **создаются заранее скрытыми в `setup()`** и только показываются (правило Liquid
Tools: ленивый `.build()` из команды/хоткея дедлочит event loop на Windows).

| Окно | Свойства | Кто показывает |
|---|---|---|
| `picker` | прозрачное, без рамки, always-on-top, skip-taskbar, не в Alt+Tab (`WS_EX_TOOLWINDOW`), `disable_drag_drop_handler`, размер по расположению + прозрачное поле `BLEED` под тень | хоткей, трей, наведение на край |
| `settings` | обычное окно Windows: системный заголовок, `decorations: true`, мин. 720×520, запоминает размер/позицию (`settings.json` → `window`), **CloseRequested → hide**, а не уничтожение | трей, `Ctrl+,` в шелфе, онбординг при первом запуске |
| `hud` | прозрачное, без рамки, always-on-top, skip-taskbar, **неактивируемое** (`WS_EX_NOACTIVATE`), click-through, ~320×56 | Rust: `hud::show(kind, text)` у курсора, автоскрытие 1.6 с |

У всех окон — `CloseRequested` перехвачен (Alt+F4 не уничтожает заранее созданные окна;
в Liquid Tools это баг шелфа).

## 3. Порядок `setup()`

1. `logx::init` (файл с ротацией в `%APPDATA%\com.liqlip.app\logs\`).
2. Плагины: `single-instance` (второй запуск → показать шелф), `autostart`, `dialog`, `opener`.
3. `settings::load` (+ миграция версии) → `manage`.
4. `store::open` (миграции схемы, WAL) → `manage`. **До создания любого окна** (окна шлют IPC сразу).
5. `capture::start` (listener-поток) · `paste::init` · `ocr::start` (W2).
6. Окна `picker`, `hud`, `settings` (скрытые).
7. `hotkeys::start` + применение `settings.hotkeys`.
8. `tray::build` (подписи из `i18n_native` по языку).
9. `theme::apply`.
10. Если `!settings.general.onboardingDone` → показать `settings` в режиме онбординга.

## 4. Данные

### 4.1. Файлы

```
%APPDATA%\com.liqlip.app\
  liqlip.db (+ -wal, -shm)     SQLite
  media\<id>.png               полноразмерные картинки
  media\thumbs\<id>.png        превью 360 px по длинной стороне
  icons\<appId>.png            иконки приложений-источников (одна на exe)
  settings.json                Settings (§9.4), с полем version
  logs\liqlip.log              без содержимого буфера
```

### 4.2. Схема SQLite v1 (`PRAGMA user_version = 1`)

Новое приложение — схема своя (неприкосновенность схемы Liquid Tools здесь не действует, но
после релиза v1 эта схема меняется **только миграциями**).

```sql
PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 3000; PRAGMA foreign_keys = ON;

CREATE TABLE apps (
  id        INTEGER PRIMARY KEY,
  exe       TEXT NOT NULL UNIQUE,      -- полный путь в нижнем регистре
  name      TEXT NOT NULL,             -- FileDescription или имя файла без .exe
  icon_path TEXT                       -- icons\<id>.png или NULL
);

CREATE TABLE folders (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  position   REAL NOT NULL,            -- ручной порядок (дробные вставки)
  created_at INTEGER NOT NULL
);

CREATE TABLE clips (
  id          INTEGER PRIMARY KEY,
  kind        TEXT NOT NULL,           -- text|link|code|color|contact|image|file
  title       TEXT,                    -- пользовательское имя (NULL = авто)
  preview     TEXT NOT NULL,           -- ≤ 512 символов для карточки
  text        TEXT,                    -- полный текст (text/link/code/color/contact), пути файлов через \n
  hash        TEXT NOT NULL UNIQUE,    -- хэш содержимого (глобальная дедупликация)
  size        INTEGER NOT NULL DEFAULT 0, -- байты, которые хранит Liqlip (НЕ размер внешних файлов)
  char_count  INTEGER, word_count INTEGER, line_count INTEGER,
  width       INTEGER, height INTEGER, -- картинки
  file_count  INTEGER,                 -- файлы
  app_id      INTEGER REFERENCES apps(id) ON DELETE SET NULL,
  created_at  INTEGER NOT NULL,        -- мс, первое копирование
  copied_at   INTEGER NOT NULL,        -- мс, последнее копирование (порядок ленты)
  copy_count  INTEGER NOT NULL DEFAULT 1,
  used_at     INTEGER,                 -- мс, последняя вставка/копия из Liqlip
  use_count   INTEGER NOT NULL DEFAULT 0,
  favorite    INTEGER NOT NULL DEFAULT 0,
  fav_order   REAL,                    -- ручной порядок в «Избранном»
  folder_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
  deleted_at  INTEGER,                 -- мягкое удаление (окно отмены), чистится в фоне
  ocr_text    TEXT,                    -- W2
  ocr_state   INTEGER NOT NULL DEFAULT 0, -- 0 не нужно/нет, 1 ждёт, 2 готово, 3 ошибка
  meta        TEXT                     -- JSON: ClipMeta (§9.2)
);
CREATE INDEX idx_clips_feed     ON clips(copied_at DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_clips_kind     ON clips(kind, copied_at DESC)    WHERE deleted_at IS NULL;
CREATE INDEX idx_clips_folder   ON clips(folder_id, copied_at DESC);
CREATE INDEX idx_clips_fav      ON clips(favorite, fav_order);
CREATE INDEX idx_clips_app      ON clips(app_id, copied_at DESC);

CREATE TABLE clip_formats (           -- богатые форматы для «Вставить как…»
  clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  format  TEXT NOT NULL,               -- html|rtf|png|files
  data    BLOB,                        -- html/rtf/список файлов+DropEffect (≤ 2 МБ)
  path    TEXT,                        -- для png — media\<id>.png
  size    INTEGER NOT NULL,
  PRIMARY KEY (clip_id, format)
);

CREATE TABLE snippets (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, body TEXT NOT NULL,
  keyword TEXT UNIQUE,                 -- зарезервировано под v2 (авторасширение)
  position REAL NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  use_count INTEGER NOT NULL DEFAULT 0
);

-- полнотекстовый поиск (external content); W1-A проверяет, что bundled SQLite собран с FTS5
-- и tokenizer 'trigram' доступен (SQLite ≥ 3.34); иначе — fallback LIKE с пометкой в отчёте
CREATE VIRTUAL TABLE clips_fts USING fts5(
  title, text, ocr_text, content='clips', content_rowid='id', tokenize='trigram'
);
-- + триггеры AFTER INSERT/UPDATE/DELETE на clips для синхронизации clips_fts
CREATE VIRTUAL TABLE snippets_fts USING fts5(name, body, content='snippets', content_rowid='id', tokenize='trigram');
```

**Хэш:** стабильный 128-битный (например, `xxh3_128` или `blake3` — выбирает W1-A, crate
проверить), от `kind` + канонического содержимого (текст — как есть; картинка — байты PNG
после нормализации; файлы — отсортированные пути в нижнем регистре).

**Правила:**
- Вставка новой записи с существующим `hash` → `copied_at = now, copy_count += 1`, если запись
  была мягко удалена — воскресить (`deleted_at = NULL`). Это «поднять наверх».
- Мягкое удаление: `deleted_at = now`; все листинги фильтруют `deleted_at IS NULL`; чистка
  (жёсткое удаление + файлы) — фоном для записей старше 2 минут после удаления.
- `undoToken` — непрозрачная строка: список id + тип операции (удаление/перенос/очистка) +
  предыдущие значения; живёт 2 минуты в памяти.
- **Ретенция** (на старте, раз в час, после вставки записи с порогом «не чаще раза в 30 с»,
  при сохранении настроек): удаляются только `favorite = 0 AND folder_id IS NULL` по
  возрасту (`copied_at`), затем по числу (оставить новейшие N), затем по суммарному
  `size` (+ размер медиа). Сниппеты не участвуют.

### 4.3. Порядок ленты и курсор

- По умолчанию: `ORDER BY copied_at DESC, id DESC`.
- `favoritesFirst`: `ORDER BY favorite DESC, copied_at DESC, id DESC`.
- Пилюля «Избранное»: `ORDER BY fav_order ASC, id ASC`.
- Поиск с текстом: тот же порядок (предсказуемость важнее релевантности), матчинг через
  `clips_fts MATCH`.
- Курсор — **непрозрачная строка** (base64 от значений ключа сортировки последней строки);
  фронт её не разбирает. Страница — 40 записей.

## 5. Захват (`capture/`)

```
listener-поток (скрытое message-only окно, AddClipboardFormatListener)
   └─ WM_CLIPBOARDUPDATE → дебаунс 30 мс → snapshot (один OpenClipboard, ретраи 10×/25 мс)
        1. приватность: Clipboard Viewer Ignore · ExcludeClipboardContentFromMonitorProcessing ·
           CanIncludeInClipboardHistory=0 · CanUploadToCloudClipboard=0  → выход
        2. пауза / «пропустить следующее» / self-copy guard → выход
        3. владелец: GetClipboardOwner → GetWindowThreadProcessId → exe (fallback: foreground)
           → игнор-лист → выход
        4. перечислить форматы (EnumClipboardFormats), прочитать ТОЛЬКО нужные, по приоритету:
           CF_HDROP(+Preferred DropEffect) → текст(CF_UNICODETEXT)+HTML Format+RTF → PNG/DIBV5/DIB
           (если есть и текст, и картинка: Office/Excel диапазон → text + png в clip_formats;
            картинка — основной вид, только если текста нет или это явный image-источник)
        5. закрыть буфер как можно раньше
   └─ отправка в worker-очередь (канал)
worker-поток:
   классификация (classify.rs) → «похоже на секрет» → тумблеры типов → кодирование PNG,
   превью-файл, метрики (символы/слова/строки) → store.insert_clip(NewClip) → событие
   clips:changed{origin:"capture"} → (W2) OCR-очередь → ретенция (с порогом)
```

- Таймаут чтения одного формата ≤ 1 с (зависшее приложение-источник с delayed rendering не
  вешает поток; формат пропускается, в лог).
- Иконка приложения: извлекается один раз на exe (`SHGetFileInfoW` → `GetIconInfo` →
  `GetDIBits` → PNG; COM на worker-потоке) — перенос из Liquid Tools
  `src-tauri/src/screenshot/mod.rs` (~строки 1001–1224).
- **Self-copy guard** — Liqlip, кладя данные в буфер, запоминает `GetClipboardSequenceNumber()`
  сразу после своей записи; listener игнорирует обновление с этим номером (надёжнее, чем
  отпечатки содержимого в Liquid Tools).
- Режим «собирать в очередь»: после успешной вставки записи worker добавляет её id в очередь
  (`paste::stack::push`).

## 6. Вставка (`paste/`)

- **Цель** (`focus.rs`): при вызове шелфа (до показа) — `GetForegroundWindow()`; отбрасываем
  свои окна (по HWND всех окон Liqlip, включая трей-меню), рабочий стол (`Progman`/`WorkerW`),
  shell-трей (`Shell_TrayWnd`). Если отброшено — цель `None` (а **не** перезапись нулём
  предыдущей). Для цели вычисляем `TargetInfo` (exe, имя, иконка, `elevated`).
- **UIPI** (`uipi.rs`): `elevated = integrity(target process) > integrity(self)`
  (`OpenProcessToken` + `GetTokenInformation(TokenIntegrityLevel)`); при отказе доступа к токену
  — считать `elevated = true`.
- **Запись в буфер** (`writer.rs`): одна транзакция `OpenClipboard/EmptyClipboard/SetClipboardData…`
  с ретраями; текст+HTML(+RTF) / только текст (plain) / PNG + CF_DIB / CF_HDROP + `Preferred
  DropEffect = DROPEFFECT_COPY` + текстовый fallback путей. Перенос писателей Liquid Tools
  `clipboard/paste.rs` с исправлением утечки HGLOBAL при неудаче `OpenClipboard`.
- **Ввод** (`input.rs`): `SendInput`. Перед `Ctrl+V` — отпустить все физически зажатые
  модификаторы (`GetAsyncKeyState` для Shift/Ctrl/Alt/Win, синтетический keyup), затем chord
  из `settings.picker.pasteKeys` для exe цели (по умолчанию Ctrl+V). `{cursor}` у сниппетов —
  N нажатий ← после вставки.
- **Поток вставки:** `picker::hide_now()` (без анимации ожидания) → `SetForegroundWindow(target)`
  → опрос `GetForegroundWindow()` до совпадения (≤ 500 мс, шаг 10 мс) → запись в буфер → 30 мс →
  `SendInput`. Если цель не стала активной → **ввод не посылаем**, данные уже в буфере,
  `hud::show(Copied)`; `PasteOutcome { result: "copied", reason: "focus_lost" }`.
  Без `SPI_SETFOREGROUNDLOCKTIMEOUT` (меняет системную настройку — запрещено).
- **Очередь вставки** (`stack.rs`): в памяти, `Vec<StackItem>`; `paste_next` вставляет верхний
  элемент в **текущее** активное окно (шелф не участвует), снимает его; события `stack:changed`.
- **Drag-out** (`dragout.rs`): перенос Liquid Tools `src-tauri/src/dragout.rs` (OLE DoDragDrop);
  текст тянется как текст, картинка — как PNG-файл из `media\`, файлы — CF_HDROP.

## 7. Окно шелфа (`picker/`)

Перенос логики Liquid Tools `src-tauri/src/screenshot/shotlib.rs` (докинг по физическому `rcWork`
монитора, строгий hover-триггер, `BLEED = 72`, отложенный `hide` 260 мс под анимацию выхода) с
переработкой состояния:

- Явная стейт-машина `Hidden → Armed(hover) → Open{mode: Summon|Hover, interactive} → Closing → Hidden`
  в одном `Mutex<PickerState>` вместо пяти статиков Liquid Tools.
- **Summon** (хоткей/трей/второй экземпляр): `paste::focus::capture_target()` → dock на монитор
  (по настройке: под курсором / основной / конкретный) → показать → **активировать** окно
  (клавиатура сразу работает) → `picker:open {mode:"summon", target, interactive:true}`.
  Повтор хоткея при открытом шелфе → закрыть.
- **Hover** (только край, если `hoverOpen`): окно неактивируемое (`set_focusable(false)`),
  `interactive:false`; `picker_make_interactive` (клик в поиск) делает его фокусируемым и
  активирует.
- **Выходы** (одна воронка `close(reason, restore_focus)`): Esc с фронта · повтор хоткея ·
  клик вне панели (опрос кнопок мыши, пока открыт) · потеря фокуса (`WindowEvent::Focused(false)`
  для interactive) · вставка (без restore — фокус уже у цели) · уход курсора (только hover).
- **Без LL-хука на Esc** — Esc обрабатывает фронт (окно активно). LL-хук нужен только движку
  хоткеев для «голых» модификаторов/особых клавиш.
- Изменение расположения/монитора/«одной строки» → `picker:layout`.
- **HUD** (`hud.rs`): `show(kind, text)` — позиционировать у курсора на его мониторе, показать без
  активации, автоскрытие 1.6 с.

## 8. Хоткеи (`hotkeys/`)

Перенос движка Liquid Tools `src-tauri/src/hotkeys.rs` (гибрид `RegisterHotKey` +
`WH_KEYBOARD_LL`; парсинг/коллизии ~строки 212–316, поток-регистратор ~646–974) **с вырезанием**
диктовки, PrintScreen-спецслучая, Esc-перехвата и `crate::Feature` Liquid Tools. Новые действия:

```rust
enum HotkeyAction { Summon, StackNext, PauseToggle, IgnoreNext }
```

- `RegisterHotKey` — для обычных сочетаний (LL-хук **не** ставится без нужды — в Liquid Tools
  ставится всегда). LL-хук — только если сочетание нельзя зарегистрировать через `RegisterHotKey`.
- Инвариант Liquid Tools: **коллбек LL-хука не делает работы** (никакого IPC/окон/I/O) — только
  обновляет состояние, решает «проглотить», планирует и возвращает (иначе Windows молча снимает
  хук по таймауту; см. Liquid Tools `docs/hotkeys.md` §2, `docs/hotkey-engine-debugging.md`).
- Конфликт (`RegisterHotKey` вернул ошибку — занято, например, Liquid Tools на `Alt + V`) →
  `settings_set` возвращает `errors: [{field:"hotkeys.summon", code:"taken"}]`, настройка не
  применяется, UI показывает ошибку.
- Запись сочетания — `hotkey_record_start/stop` + событие `hotkeys:captured`; фронтовая часть —
  перенос `src/lib/hotkeyCapture.ts` (+ тест). Нотация как в Liquid Tools (`"Alt + V"`).
- Раскладка: регистрация по VK-кодам (физические клавиши) — русская раскладка не ломает `Alt+V`.

## 9. IPC-контракт

TS — `src/lib/ipc/types.ts`; Rust — `src-tauri/src/model.rs` (`#[serde(rename_all = "camelCase")]`).
Время — **миллисекунды Unix** (`number`/`i64`). Id — `number`/`i64`.

### 9.1. Перечисления

```ts
export type Kind = 'text' | 'link' | 'code' | 'color' | 'contact' | 'image' | 'file';
export type Format = 'text' | 'html' | 'rtf' | 'png' | 'files';
export type PasteMode = 'auto' | 'plain' | { format: Format };
export type Position = 'center' | 'left' | 'right' | 'top' | 'bottom';
export type ChangeOrigin = 'capture' | 'user' | 'import' | 'retention';
```

### 9.2. Записи

```ts
export interface AppInfo { id: number; exe: string; name: string; iconUrl: string | null; clipCount?: number }

export interface ClipMeta {
  url?: string; host?: string;                       // link; html SourceURL
  color?: { hex: string; rgb: string; hsl: string }; // color
  contact?: 'email' | 'phone';
  ext?: string;                                      // file: расширение первого файла
  truncated?: boolean;                               // текст усечён до 2 МБ
}

export interface Clip {
  id: number; kind: Kind; title: string | null; preview: string;
  appId: number | null;
  createdAt: number; copiedAt: number; copyCount: number; usedAt: number | null; useCount: number;
  favorite: boolean; folderId: number | null;
  size: number; width: number | null; height: number | null;
  charCount: number | null; fileCount: number | null;
  thumbUrl: string | null;          // http://liq.localhost/thumb/<id>?v=<n> для image
  formats: Format[];                // доступные для «Вставить как…»
  meta: ClipMeta;
  ocr: 'none' | 'pending' | 'done' | 'error';
}

export interface ClipDetail extends Clip {
  text: string | null;              // полный текст
  wordCount: number | null; lineCount: number | null;
  files: { path: string; exists: boolean; size: number | null }[] | null;
  imageUrl: string | null;          // http://liq.localhost/media/<id>
  ocrText: string | null;
}

export interface Folder { id: number; name: string; position: number; count: number }
export interface Snippet { id: number; name: string; body: string; keyword: string | null;
  position: number; createdAt: number; updatedAt: number; useCount: number }
export interface SnippetInput { id?: number; name: string; body: string; keyword?: string | null }
```

### 9.3. Запросы ленты

```ts
export interface Filter {
  kinds?: Kind[]; favorites?: boolean; folderId?: number; appIds?: number[];
  since?: number; until?: number;
}
export interface FeedQuery {
  text: string;            // свободный текст БЕЗ токенов (токены разбирает фронт: picker/query.ts)
  exclude: string[];       // слова с минусом
  filter: Filter;
  cursor: string | null;   // непрозрачный
  limit: number;           // 40
}
export interface Page { items: Clip[]; nextCursor: string | null }
export interface Counts { all: number; kinds: Record<Kind, number>; favorites: number; snippets: number;
  folders: Record<number, number> }
```

### 9.4. Настройки

```ts
export interface Settings {
  version: 1;
  general: { language: 'system' | 'en' | 'ru'; theme: 'system' | 'light' | 'dark';
             autostart: boolean; onboardingDone: boolean };
  picker: {
    position: Position; centerRow: boolean; monitor: 'cursor' | 'primary' | string; // string = device name
    hoverOpen: boolean; hints: boolean; favoritesFirst: boolean;
    clickAction: 'paste' | 'copy'; closeOnCopy: boolean; alwaysPlain: boolean; bumpOnPaste: boolean;
    rememberSearch: 'never' | '60s' | 'always';
    mergeSeparator: 'newline' | 'space' | 'tab' | 'none';
    pasteKeys: { exe: string; chord: 'ctrl+v' | 'shift+insert' | 'ctrl+shift+v' }[];
  };
  hotkeys: { summon: string; stackNext: string; pauseToggle: string; ignoreNext: string }; // "" = не задан
  storage: { retention: '1d' | '1w' | '1m' | '3m' | '6m' | '1y' | 'forever';
             maxEntries: number; maxSizeMb: number;          // 0 = без ограничения
             types: Record<Kind, boolean> };
  privacy: { respectMarkers: boolean; ignoredApps: string[]; secretPatterns: boolean };
  stack: { newlineAfterPaste: boolean };
  ocr: { enabled: boolean; language: 'auto' | string };
  window: { settingsBounds: { x: number; y: number; w: number; h: number } | null };
}
export interface SettingsResult { settings: Settings; errors: { field: string; code: 'taken' | 'invalid' }[] }
```

Дефолты: language system · theme system · autostart **true** · position center · centerRow false ·
monitor cursor · hoverOpen false · hints true · favoritesFirst false · clickAction paste ·
closeOnCopy true · alwaysPlain false · bumpOnPaste false · rememberSearch 60s ·
mergeSeparator newline · pasteKeys [] · hotkeys summon `"Alt + V"`, stackNext `"Alt + Shift + V"`,
остальные `""` · retention 3m · maxEntries 5000 · maxSizeMb 1024 · все типы true ·
respectMarkers true · ignoredApps `[keepass.exe, keepassxc.exe, 1password.exe, bitwarden.exe]` ·
secretPatterns true · newlineAfterPaste false · ocr enabled true, language auto.

### 9.5. Прочие типы

```ts
export interface TargetInfo { exe: string; name: string; title: string; iconUrl: string | null; elevated: boolean }
export interface PasteRequest { ids: number[]; mode: PasteMode; separator?: string; keepOpen?: boolean }
export interface TextPasteRequest { text: string; plain: boolean; caretBack?: number; keepOpen?: boolean }
export interface PasteOutcome { result: 'pasted' | 'copied' | 'failed';
  reason?: 'no_target' | 'elevated' | 'focus_lost' | 'clipboard_busy' | 'not_found' }
export interface CaptureStatus { pausedUntil: number | null; /* -1 = до возобновления */ ignoreNext: boolean; collectToStack: boolean }
export interface StackItem { key: string; clipId: number; kind: Kind; preview: string }
export interface Stack { items: StackItem[] }
export interface StorageStats { clips: number; dbBytes: number; mediaBytes: number }
export interface ImportProbe { found: boolean; path: string | null; clips: number; favorites: number; folders: number }
export interface ImportReport { imported: number; skipped: number; folders: number; errors: number }
```

### 9.6. Команды (`#[tauri::command]`, snake_case; в TS — camelCase-обёртки в `ipc/commands.ts`)

| Команда | Аргументы → результат | Модуль-владелец |
|---|---|---|
| `clips_list` | `q: FeedQuery` → `Page` | store |
| `clips_counts` | → `Counts` | store |
| `clip_detail` | `id` → `ClipDetail` | store |
| `clip_set_favorite` | `ids, value` → `()` | store |
| `clip_move_favorite` | `id, beforeId: number\|null` → `()` | store |
| `clip_set_folder` | `ids, folderId: number\|null` → `{ undoToken }` | store |
| `clip_rename` | `id, title: string\|null` → `Clip` | store |
| `clip_update_text` | `id, text` → `Clip` | store |
| `clip_create_text` | `text, folderId?: number` → `Clip` (не проходит через захват) | store |
| `clips_delete` | `ids` → `{ undoToken }` | store |
| `clips_undo` | `token` → `boolean` | store |
| `clips_delete_recent` | `minutes` → `number` | store |
| `clips_clear` | `scope: 'history'\|'all'` → `{ undoToken: string\|null }` | store |
| `folders_list` / `folder_create` / `folder_rename` / `folder_move` / `folder_delete` | `…` / `name`→`Folder` / `id,name` / `id,beforeId` / `id, withClips` → `{undoToken}` | store |
| `snippets_list` | `text` → `Snippet[]` | store |
| `snippet_save` / `snippet_delete` / `snippet_mark_used` | `SnippetInput`→`Snippet` / `id` / `id` | store |
| `apps_list` | → `AppInfo[]` (источники с числом записей) | store |
| `storage_stats` | → `StorageStats` | store |
| `clip_paste` | `PasteRequest` → `PasteOutcome` | paste |
| `clip_copy` | `{ ids, mode, separator? }` → `PasteOutcome` | paste |
| `text_paste` | `TextPasteRequest` → `PasteOutcome` | paste |
| `text_copy` | `text` → `()` | paste |
| `clip_open` / `clip_open_with` / `clip_reveal` | `id` → `()` | paste |
| `clip_drag_out` | `ids` → `'dropped'\|'cancelled'` | paste |
| `stack_get` / `stack_add` / `stack_remove` / `stack_move` / `stack_reverse` / `stack_clear` | → `Stack` | paste |
| `stack_paste_all` | `separator` → `PasteOutcome` | paste |
| `stack_paste_next` | → `PasteOutcome` | paste |
| `capture_status` / `capture_pause` / `capture_resume` / `capture_ignore_next` / `capture_set_collect` | → `CaptureStatus`; `minutes: number\|null` (null = до возобновления); `value` | capture |
| `apps_running` | → `{ exe, name, iconUrl }[]` | capture |
| `picker_hide` | `restoreFocus` → `()` | picker |
| `picker_make_interactive` | → `()` | picker |
| `picker_target` | → `TargetInfo \| null` | picker |
| `settings_open` | `section?: string` → `()` (показать окно настроек + `settings:navigate`) | picker |
| `monitors_list` | → `{ id: string; name: string; primary: boolean; width: number; height: number; scale: number }[]` (`id` — имя устройства, то, что пишется в `settings.picker.monitor`) | picker |
| `settings_get` / `settings_set` | → `Settings` / `Settings` → `SettingsResult` | settings (+ хуки модулей) |
| `hotkey_record_start` / `hotkey_record_stop` | `()` | hotkeys |
| `app_info` | → `{ version, dataDir, logDir }` | settings |
| `open_path` | `which: 'data'\|'logs'` → `()` | settings |
| `pick_exe` | → `string \| null` (диалог) | settings |
| `ocr_status` / `ocr_reindex` / `clip_ocr` | → `{ available, languages, pending }` / `()` / `id` → `string` | ocr (W2) |
| `import_lt_probe` / `import_lt_run` | → `ImportProbe` / `ImportReport` | import (W2) |
| `backup_export` / `backup_import` | → `string\|null` / `mode: 'replace'\|'merge'` → `ImportReport` | import (W2) |

Ошибки команд — `Result<T, String>` с машинным кодом в начале строки (`"not_found: …"`,
`"busy: …"`, `"invalid: …"`); фронт показывает локализованный текст по коду.

### 9.7. События (`emit_to` адресно; ⚑ = во все окна)

| Событие | Payload | Кто шлёт → кому |
|---|---|---|
| `clips:changed` | `{ origin: ChangeOrigin; change: 'added'\|'updated'\|'deleted'\|'reset'; ids?: number[] }` | store/capture → picker, settings |
| `picker:open` | `{ mode: 'summon'\|'hover'; target: TargetInfo \| null; interactive: boolean }` | picker → picker |
| `picker:close` | `{}` | picker → picker (освободить строки через 260 мс) |
| `picker:layout` | `{ position; centerRow; width; height; bleed }` | picker → picker |
| `capture:status` | `CaptureStatus` | capture → picker, settings |
| `stack:changed` | `Stack` | paste → picker |
| `settings:changed` ⚑ | `Settings` | settings → все |
| `settings:navigate` | `{ section: string }` | picker (`settings_open`) → settings |
| `hotkeys:captured` | `{ combo: string }` | hotkeys → settings |
| `hud:show` | `{ kind: 'copied'\|'stack_empty'\|'paused'\|'elevated'; text?: string }` | picker/paste → hud |
| `ocr:progress` | `{ done: number; total: number }` | ocr → settings |
| `import:progress` | `{ phase: string; done: number; total: number }` | import → settings |
| `theme://changed` ⚑ | `'light'\|'dark'` | theme → все |

**Фронт отличает свои изменения от чужих по `origin`:** плашка «Новые записи» — только для
`origin: 'capture'`.

### 9.8. URI-схема `liq`

`register_asynchronous_uri_scheme_protocol("liq", …)` (по образцу `ltthumb` Liquid Tools,
`lib.rs:198–226`); в WebView2 адрес `http://liq.localhost/<path>`:
`/thumb/<id>?v=<n>` · `/media/<id>` · `/icon/<appId>` — чтение файла в `spawn_blocking`,
`Cache-Control: immutable` (версия в query), 404 для отсутствующих.

## 10. Фронтенд

- **Адаптер IPC:** `ipc/commands.ts` вызывает `invoke`, если `isTauri()`, иначе — `ipc/mock/`
  (in-memory сид ~300 записей всех типов, папки, сниппеты, имитация задержки 30–80 мс и
  событий). Благодаря этому `pnpm dev` показывает рабочий шелф и настройки **в обычном
  браузере** — так UI проверяется без нативного приложения.
- **Шелф** (`windows/picker/`): корень `Picker.svelte` тонкий; состояние в `*.svelte.ts`
  (`feed` — страницы/курсор/виртуализация, `selection` — текущая/мультивыбор, `keyboard` —
  карта клавиш по `event.code`, `overlays` — просмотр/меню/очередь); компоненты `SearchBar`
  (чипы токенов, автодополнение), `Pills`, `Grid` (виртуализация — перенос `layout.ts` Liquid
  Tools + 2D-навигация), `CardShell`/`cards/*` (перенос), `CardActions` (3 + ⋯), `Footer`
  (подсказки, цель, пауза, очередь), `EmptyState`/скелетоны. Чистая логика — в `.ts` с тестами:
  `query.ts` (токены → `FeedQuery`), `layout.ts`, `gridNav.ts`, `transforms.ts`, `snippetExpand.ts`,
  `format.ts`/`preview.ts`.
- **Настройки** (`windows/settings/`): `Settings.svelte` — сайдбар + секции (`sections/*.svelte`),
  онбординг (`onboarding/*.svelte`); стор настроек — один `settings.svelte.ts` поверх
  `settings_get/set` + `settings:changed`.
- **Тема/язык** — из `Settings.general` (бэкенд авторитетен; рассылка `settings:changed`).

## 11. Дизайн-система

Перенос из Liquid Tools **без изменений**: `src/app.css` (`@theme`, светлая/тёмная палитры,
радиусы, тени, моушен, утилита `squircle`), правила монохром-лока (цвет — только в
пользовательском контенте: ColorCard), «никаких границ/разделителей» (hairline/divider
прозрачны), капсулы — `border-radius: 999px` без `squircle`, шрифты Inter + JetBrains Mono
(моно — только функционально), моушен 150–250 мс ease-out, жидкая кривая
`cubic-bezier(.34,1.24,.44,1)`, `prefers-reduced-motion`. Справочник — Liquid Tools
`docs/design-system.md`. Окно настроек — те же токены на системном заголовке.

## 12. Проверка (гейты)

| Гейт | Команда |
|---|---|
| Типы Svelte/TS | `pnpm check` → 0 ошибок, 0 предупреждений |
| Node-тесты | `pnpm test` (`node --test tests/`) |
| Сборка фронта | `pnpm build` (единственное, что резолвит импорты иконок) |
| Rust | `cargo check --manifest-path src-tauri/Cargo.toml`; `cargo test …` если работает (W0) |
| UI в браузере | `pnpm dev` (порт **1430**, strict — не конфликтует с Liquid Tools 1420) + мок-адаптер |
| Нативно | `pnpm tauri dev` — **только Eddy** (правило воркспейса: агенты нативное приложение не запускают, пока Eddy не разрешит) |

## 13. Владение файлами и правила для агентов

- **Файлы-узлы координатора:** `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`,
  `src-tauri/capabilities/*.json`, `src-tauri/src/model.rs` + `src/lib/ipc/types.ts` (контракт),
  `src/lib/i18n/{en,ru}.ts` (агенты дописывают только свой префикс), `src/app.css`.
- Контракт: агент может **аддитивно** добавить поле/команду, если без этого задача не решается,
  — в обоих файлах (`model.rs` + `types.ts`) и с явным пунктом в отчёте. Переименовывать и
  удалять — нельзя. Регистрацию новой команды в `lib.rs` агент сдаёт сниппетом.
- `Cargo.toml` / `package.json`: агент может добавить зависимость, нужную его модулю
  (с комментарием-областью); конфликты при слиянии разрешает координатор.
- После W0 каждая команда §9.6 уже зарегистрирована в `lib.rs` и имеет заглушку
  `Err("unimplemented: …")` в `src-tauri/src/<module>/commands.rs` — агенты W1 только
  наполняют свои модули.

## 14. Карта переноса из Liquid Tools

База: `C:\Coding\My Projects\LiquidTools\liquid-tools-app\` (ветка `studio-rework`; в рабочем
дереве есть незакоммиченные правки — **только читать**, ничего там не менять).

| Что | Откуда | Как |
|---|---|---|
| Токены, шрифты, `squircle` | `src/app.css` | копия |
| Общие компоненты, иконки, моушен, i18n-инфра, тема, flash | `src/lib/components/**`, `src/lib/heroicons/`, `src/lib/icons.ts`, `src/lib/motion/`, `src/lib/i18n/index.svelte.ts`, `src/lib/theme.svelte.ts`, `src/lib/flash.svelte.ts` | копия + чистка неиспользуемого |
| Запись сочетаний (TS) | `src/lib/hotkeyCapture.ts`, `src/lib/hotkey.ts`, `src/lib/hotkeyError.ts`, `tests/hotkeyCapture.test.mjs` | копия |
| Шелф | `src/windows/shotlibrary/**` (`ShotLibrary.svelte`, `CardShell`, `CardActions`, `cards/*`, `layout.ts`, `format.ts`, `preview.ts`), тесты `tests/library{Layout,Preview,Pills}.test.mjs` | разобрать на компоненты, переписать состояние |
| Чтение/классификация буфера | `src-tauri/src/clipboard/capture.rs`, `monitor.rs` | перенос классификации, чтение — заново (listener, один OpenClipboard) |
| Запись в буфер, guard, вставка | `src-tauri/src/clipboard/paste.rs`, `mod.rs` (`set_clipboard_image_native`, paste-поток) | перенос писателей, ввод — заново (SendInput) |
| Окно шелфа | `src-tauri/src/screenshot/shotlib.rs` | перенос докинга/hover, стейт-машина заново |
| Drag-out | `src-tauri/src/dragout.rs` | перенос (убрать зависимость от `screenshot::wait_file_ready`) |
| Иконки приложений | `src-tauri/src/screenshot/mod.rs` ~1001–1224 | перенос |
| Хоткеи | `src-tauri/src/hotkeys.rs` | перенос движка, вырезать диктовку/PrintScreen/Esc/Feature |
| Тема, логи, fs, автозапуск, single-instance | `theme.rs`, `logx.rs`, `fsx.rs`, `autostart.rs`, `lib.rs:168` | копия |
| URI-схема | `lib.rs:198–226` (`ltthumb`) | по образцу |
| OCR (W2) | `git -C liquid-tools-app show 0295998:src-tauri/src/ocr.rs` (удалён в `0c2a612`) | перенос |
| Импорт (W2) | схема Liquid Tools — `docs/library.md` §4 + `clipboard/store.rs`; база `%APPDATA%\com.liquidtools.app\clipboard\clipboard.db` | читать read-only |
