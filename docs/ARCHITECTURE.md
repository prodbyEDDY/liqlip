# Liqlip — архитектура и IPC-контракт

> Технический дизайн v1. Что строим — [SPEC.md](SPEC.md); статус — [ROADMAP.md](ROADMAP.md).
> Это **контракт** для параллельных агентов: типы, команды, события, владение файлами.
> Менять контракт может только координатор (агенты — аддитивно и с пометкой в отчёте, см. §13).
> Замер: 2026-11-26 (W0: фундамент собран — каркас, контракт, заглушки всех команд, мок, settings;
> гейты зелёные; нативно не запускалось).

## 0. Стек

| Слой | Выбор | Примечание |
|---|---|---|
| Оболочка | **Tauri 2** (как Liquid Tools, 2.11.x) | `tray-icon`; плагины: `single-instance`, `autostart`, `dialog` (выбор exe, экспорт/импорт), `opener` (открыть ссылку/файл) |
| UI | **Svelte 5 (runes)** + **Vite MPA** + **Tailwind v4** + TypeScript | одна HTML-точка на окно, как в Liquid Tools |
| Шрифты | `@fontsource-variable/inter`, `@fontsource-variable/jetbrains-mono` | self-hosted |
| Иконки | Heroicons outline через локальный `src/lib/heroicons/` + `Ico` (как Liquid Tools после 2026-09-22) | |
| Rust | `rusqlite` **0.43.0** `bundled` (libsqlite3-sys 0.40.0 → SQLite **3.54.1**, собрано с `SQLITE_ENABLE_FTS5`; на rustc 1.99 собирается — пин `=0.38` из Liquid Tools не нужен), `windows` 0.62.2 (Win32/WinRT; фичи — по назначению, с комментарием `# W1-X`), `image` 0.25.10 (`png`), `xxhash-rust` 0.8.21 (`xxh3`, общий хэш — §15), `serde` 1.0.228, `serde_json` 1.0.150, `parking_lot` 0.12.5, `base64` 0.22, `tokio` 1.52 (`sync`). `arboard` W0 **не** добавлял — чтение/запись своими Win32-вызовами (решают W1-B/W1-C) | `enigo` **не** используем — `SendInput` напрямую |
| Тесты | `node:test` + `esbuild` (как Liquid Tools) для чистой TS-логики; **`cargo test` в крейте `src-tauri` работает** (W0, 2026-11-26: 24 теста, тест-бинарник стартует со всеми плагинами) — Rust TDD доступен, отдельный `liqlip-core` не нужен (§12) | |

Toolchain на машине (2026-11-25): rustc/cargo 1.99, node 24.21, pnpm 12.6.

**Фактические версии (W0, 2026-11-26).** Rust: `tauri` **2.11.2** (`~2.11`), `tauri-build` 2.6.2,
`tauri-plugin-single-instance` 2.4.2, `-autostart` 2.5.1, `-dialog` 2.7.1, `-opener` 2.5.4 — ровно набор
Liquid Tools. `Cargo.lock` засеян из лока Liquid Tools (cargo сам вычистил лишнее): свежий резолв
`~2.11` берёт `tauri` 2.11.6 вместе с `tauri-runtime` 2.12.4, и `tauri` не компилируется (27 ошибок
несовместимости API). Не делать `cargo update` для tauri-крейтов без проверки; переход на 2.12 —
отдельной задачей вместе с `@tauri-apps/api`/`cli`. JS: `svelte` 5.59.1, `vite` 8.3.8, `tailwindcss` /
`@tailwindcss/vite` 4.3.5, `@sveltejs/vite-plugin-svelte` 7.3.2, `typescript` 6.0.3, `svelte-check` 4.7.8,
`esbuild` 0.28.2, `@tauri-apps/api` 2.11.1, `@tauri-apps/cli` 2.11.5, `bits-ui` 2.22.0 (Dropdown),
`@fontsource-variable/inter` / `jetbrains-mono` 5.3.0. pnpm 12 по умолчанию блокирует build-скрипты —
`pnpm-workspace.yaml` разрешает только `esbuild`.

**FTS5 + trigram** проверены тестом `store::sqlite_probe` (external-content таблица как в §4.2,
регистронезависимый поиск по подстроке для кириллицы и латиницы).

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
        runtime.ts · errors.ts  isTauri() · код ошибки → локализованный текст (common.error.*)
        mock/                   браузерный адаптер: index (роутер) · bus (события, флаги URL, задержка) ·
                                seed · clips (store+paste+stack+симулятор захвата) · settings · system
      motion/ · theme.svelte.ts · appearance.ts · flash.svelte.ts · hotkey.ts · hotkeyCapture.ts · hotkeyError.ts
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
      content.rs                общий хэш + правила содержимого (превью, усечение, метрики) — §15
      events.rs                 типизированные эмиттеры всех событий §9.7 (имена + адресация)
      tray.rs · theme.rs · logx.rs · fsx.rs · autostart.rs · i18n_native.rs
    tauri.conf.json · capabilities/default.json · icons/ (временные, PQ-01)
  tests/                        *.test.mjs (node:test) · helpers/bundle.mjs (esbuild → ESM) ·
                                fixtures/settings.default.json (дефолты: Rust ≡ мок)
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

**Как создано в W0** (`lib.rs::build_windows`): `picker` — `transparent`, `decorations(false)`,
`shadow(false)`, `always_on_top`, `skip_taskbar`, `resizable(false)`, `focusable(true)`,
`disable_drag_drop_handler`, 904×784 (760×640 + `BLEED` 72 с каждой стороны); `hud` — то же без
drag-handler, `focusable(false)` (в tao это `WS_EX_NOACTIVATE`), 320×56, `set_ignore_cursor_events(true)`
(click-through); `settings` — системный заголовок, 960×680, мин. 720×520, `center`, заголовок из
`i18n_native` (меняется с языком), `disable_drag_drop_handler` (HTML5 DnD для сниппетов/папок). Геометрия
`settings` сохраняется при скрытии и на выходе (`settings.window.settingsBounds`, **физические px**:
внешняя позиция + внутренний размер) и восстанавливается, только если полоса заголовка видна на
каком-то мониторе. **Для W1-D:** `skip_taskbar` (tao: `ITaskbarList::DeleteTab`) не убирает окно из
Alt+Tab, а tao пересчитывает `GWL_EXSTYLE` из своих флагов при каждом show/hide/`set_focusable`/
`set_ignore_cursor_events` (`tao/src/platform_impl/windows/window_state.rs::apply_diff`) — вручную
добавленный `WS_EX_TOOLWINDOW` надо переустанавливать после этих вызовов.

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

Реализовано в W0 ровно в этом порядке (`lib.rs::run`); там, где модуль ещё заглушка, вызывается
заглушка §15 (`capture::start`, `paste::init`, `ocr::start`, `picker::init`, `hotkeys::start`,
`picker::show_settings(Some("onboarding"))`). `Store::open` → `Err` прерывает запуск (W1-A делает
открытие устойчивым: битая база отодвигается и создаётся заново). На `RunEvent::Exit` сохраняется
геометрия окна настроек.

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

**Хэш** (W0, `src-tauri/src/content.rs`, §15): XXH3-128 (`xxhash-rust`), 32 hex-символа, от
`"liqlip:1\0" ‖ семейство ‖ "\0" ‖ каноническое содержимое`. Семейство, а не точный `kind`
(`text` для text/link/code/color/contact, `image`, `file`) — чтобы улучшенный классификатор в будущей
версии поднимал старую запись, а не плодил двойник. Канон: текст — как есть (после усечения до 2 МБ);
картинка — `width u32 LE ‖ height u32 LE ‖ RGBA8` (пиксели, а не байты PNG — не зависит от версии
энкодера); файлы — пути в нижнем регистре, отсортированы, через `\n`. Значение закреплено тестом
`content::tests::hash_is_stable` — менять формат после релиза нельзя.

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

> Замер: 2026-11-26 (W1-C, ветка `w1/paste`): модуль реализован целиком; `cargo test` — 49 тестов
> чистой логики модуля (73 в крейте); нативно не запускалось (PQ-02) — фокус и ввод проверяются по
> чек-листу из отчёта W1-C на INT-1.

Файлы: `focus.rs` (цель) · `uipi.rs` (уровни целостности) · `writer.rs` (транзакция буфера + байтовые
форматы) · `input.rs` (`SendInput`) · `compose.rs` (записи → содержимое буфера) · `flow.rs` (шаги
вставки) · `stack.rs` (очередь) · `open.rs` (открыть / открыть с помощью / показать) · `dragout.rs`
(OLE drag) · `commands.rs` (§9.6; всё блокирующее — в `spawn_blocking`).

### 6.1. Цель (`focus.rs`)

`capture_target()` (зовёт `picker::summon` **до** показа): `GetForegroundWindow()` → решение
`classify()` (покрыто тестом):
- **сам шелф** на переднем плане (повторный summon во время анимации закрытия, возврат после
  `keepOpen`) → **прежняя цель сохраняется** — никогда не затирается «ничем» (баг Liquid Tools);
- любое другое своё окно (по PID окна и PID его корневого владельца — настройки, трей-меню,
  всплывашки WebView2), рабочий стол (`Progman`/`WorkerW`), панели задач (`Shell_TrayWnd`,
  `Shell_SecondaryTrayWnd`, переполнение трея), невидимое/свёрнутое окно → цели нет (`None`,
  Enter копирует — SPEC §3.1);
- иначе — новая цель `Target { hwnd, pid, info: TargetInfo }`.

`TargetInfo`: `exe` — полный путь в нижнем регистре (`QueryFullProcessImageNameW`); `name` —
`FileDescription` из ресурса версии (`VerQueryValueW`, кэш на exe; иначе имя файла); `title` —
`GetWindowTextW` (чужому окну не шлёт `WM_GETTEXT` — зависшая цель не блокирует); `iconUrl` —
`capture::app_icon_data_url(exe)` (§15); `elevated` — §6.2. UWP-окна (`ApplicationFrameHost.exe`)
описываются по процессу дочернего CoreWindow. `current_target()` — та же цель, если окно живо,
видимо, не свёрнуто и принадлежит тому же PID (защита от переиспользованного HWND).

### 6.2. UIPI (`uipi.rs`)

`SendInput` «is subject to UIPI», а заблокированный вызов ничего не сообщает
([SendInput](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput)) — поэтому
уровни сравниваются заранее: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` →
`OpenProcessToken(TOKEN_QUERY)` → `GetTokenInformation(TokenIntegrityLevel)` → последний
sub-authority SID метки (low 0x1000 · medium 0x2000 · high 0x3000 · system 0x4000).
`elevated = target > self`; цель, которую нельзя опросить, — `elevated = true` (copy-only — безопасная
сторона). Если `SendInput` всё же вставил 0 событий — исход `copied/elevated` + HUD `elevated`.

### 6.3. Запись в буфер (`writer.rs`)

- **Владелец буфера** — скрытое message-only окно (`HWND_MESSAGE`) на своём потоке с циклом
  сообщений: при `OpenClipboard(NULL)` «EmptyClipboard sets the clipboard owner to NULL; this causes
  SetClipboardData to fail» ([OpenClipboard](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-openclipboard)),
  а не качающий сообщения владелец задержал бы чужой `EmptyClipboard` (`WM_DESTROYCLIPBOARD`).
  Создаётся в `paste::init`.
- **Одна транзакция**: все HGLOBAL выделяются и форматы регистрируются **до** открытия; `OpenClipboard`
  — 11 попыток с бэкоффом 0/5/10/20/30/40/50/60/80/100/100 мс (≈ 0,5 с); `EmptyClipboard`;
  `SetClipboardData` по порядку; `CloseClipboard`. Блок, не принятый системой, освобождается
  (`OwnedGlobal::drop`) на любом пути ошибки — утечки Liquid Tools нет. Сразу после `CloseClipboard`
  — `capture::mark_own_write(GetClipboardSequenceNumber())` (и при частичной неудаче: буфер уже
  изменён). Успех = принят обязательный формат; необязательные — по возможности.
- **Порядок форматов — от самого описательного** ([Clipboard Formats](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#multiple-clipboard-formats)):
  текст — `HTML Format` → `Rich Text Format` → `CF_UNICODETEXT` (обяз.); картинка — `PNG` (файл
  `media\<id>.png` как есть) → `CF_DIB` (обяз.; **одно** декодирование PNG, полупрозрачность
  сведена на белый — CF_DIB альфу не несёт, альфу берут из PNG); файлы — `CF_HDROP` (обяз.;
  DROPFILES `pFiles=20, fWide=1`, пути UTF-16 с двойным NUL) → `Preferred DropEffect = DROPEFFECT_COPY`
  (вставка из истории никогда не перемещает оригиналы) → `CF_UNICODETEXT` со списком путей (CRLF).
- **CF_HTML**: заголовок `Version:0.9` + `StartHTML/EndHTML/StartFragment/EndFragment` — байтовые
  смещения от начала данных, 10 цифр с ведущими нулями (фиксированная длина заголовка);
  `StartFragment` — сразу после `<!--StartFragment-->`, `EndFragment` — на `<!--EndFragment-->`;
  UTF-8, NUL вне `EndHTML` ([HTML Clipboard Format](https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format)).
  Маркеры из хранилища сохраняются (терпимы к пробелам `<!-- StartFragment -->`); нет маркеров —
  вставляются внутрь `<body>` или документ оборачивается; если в данных остался заголовок CF_HTML —
  берётся только контекст. `SourceURL` не пишется (в `PastePayload` его нет).

### 6.4. Что кладём (`compose.rs`)

| Режим | Одна запись | Несколько записей |
|---|---|---|
| `auto` | текстовые виды: текст + HTML + RTF (`picker.alwaysPlain` → только текст); картинка: PNG + DIB; файлы: CF_HDROP | все не-картинки — файлы → **один** CF_HDROP (объединение путей без дублей); иначе текст через разделитель (файлы — путями); картинки пропускаются → `reason: 'partial'`; только картинки → первая картинка + `partial` |
| `plain` | только `CF_UNICODETEXT` (файлы — путями); у картинки текста нет → вставляется картинка | текст через разделитель, картинки пропускаются (`partial`) |
| `{format}` | ровно этот вид (+ текст как fallback для HTML/RTF); нет формата → `invalid` | `text` = plain; `files` — объединение файловых записей (`partial`, если были другие); `html/rtf/png` → `invalid` |

Разделитель: из запроса (одиночный LF → CRLF), иначе `picker.mergeSeparator` (`\r\n` / пробел / таб /
ничего). Удалённые записи пропускаются; все удалены → `failed/not_found`; PNG пропал из `media\` →
`failed/not_found`. Использование (`store.mark_used(ids, picker.bumpOnPaste)` + `clips:changed
{origin:'user', change:'updated'}`) отмечается для реально вставленных/скопированных записей.

### 6.5. Ввод (`input.rs`)

Один `SendInput` на вставку (события «are not interspersed» с пользовательскими):
1. если физически зажат Alt или Win — нажатие неназначенной клавиши VK `0xE8` («Unassigned» в
   [Virtual-Key Codes](https://learn.microsoft.com/windows/win32/inputdev/virtual-key-codes)): иначе
   отпускание Alt было бы «голым» → меню цели (Win → «Пуск»); приём AutoHotkey «menu mask key», как
   `dictation::mask_bare_modifier` в Liquid Tools;
2. keyup всех физически зажатых L/R Shift/Ctrl/Alt/Win (`GetAsyncKeyState`);
3. chord: `Ctrl+V` | `Shift+Insert` | `Ctrl+Shift+V`;
4. keydown тех же модификаторов обратно + ещё одна маска, если среди них Alt/Win: логическое
   состояние снова равно физическому, поэтому, удерживая `Alt+Shift` и нажимая `V`, пользователь
   вставляет элемент за элементом, а не печатает «v»; финальное физическое отпускание не «голое»
   (нет меню, «Пуска», переключения раскладки `Alt+Shift`).

Клавиши — **VK + скан-код** без `KEYEVENTF_SCANCODE`: приложения ловят Ctrl+V по VK (`'V'` и в
русской раскладке), скан-код — `MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX, <раскладка потока цели>)`,
старший байт `0xE0` → `KEYEVENTF_EXTENDEDKEY` (Insert, стрелки, правые Ctrl/Alt, Win; запасная
таблица, если перевода нет). `dwExtraInfo = 0x4C514C50` («LQLP») — LL-хук хоткеев (W1-D) может
игнорировать наш ввод. Частичная вставка батча → keyup модификаторов chord'а.

**Chord по приложению**: `settings.picker.pasteKeys` (запись с `\`/`/` сравнивается с полным путём,
иначе с именем файла, без учёта регистра) → встроенные: `mintty.exe` и `putty.exe` → `Shift+Insert`
(сверено: [mintty(1)](https://mintty.github.io/mintty.1.html) — «Shift+Ins: Paste», Ctrl+V только
с `CtrlExchangeShift`; [PuTTY §3.1.1](https://the.earth.li/~sgtatham/putty/0.83/htmldoc/Chapter3.html)
— Shift-Ins) → `Ctrl+V`. Для Windows Terminal правило не нужно: по умолчанию `ctrl+v`, `ctrl+shift+v`,
`shift+insert` ([WT actions](https://learn.microsoft.com/windows/terminal/customize-settings/actions));
консоль Windows 10/11 — «Enable Ctrl key shortcuts» включено по умолчанию.

После вставки (`After`, через 40 мс, только во всё ещё активную цель): `caretBack` → `←` × n (≤ 5000)
для `{cursor}`; `stack.newlineAfterPaste` → `Enter`.

### 6.6. Поток (`flow.rs`)

Чистый планировщик `plan(origin, route, closeOnCopy) → [Step]` (порядок закреплён тестами) +
исполнитель; вставки сериализованы (вторая ждёт первую).

| Откуда / цель | Шаги |
|---|---|
| шелф, цель вставляемая | `Write → WaitKeys → FocusTarget → HideShelf → Settle(30 мс) → Inject → After → HudIfNotPasted` |
| шелф + `keepOpen` | `Write → WaitKeys → BeginHandoff → FocusTarget → Settle → Inject → After → ReturnToShelf → EndHandoff` |
| шелф, цель elevated | `Write → WaitKeys → FocusTarget → HideShelf → Hud(elevated)` → `copied/elevated` (фокус возвращён окну админа — Ctrl+V одним нажатием; `SetForegroundWindow` UIPI не блокирует, ввод — блокирует) |
| шелф, цели нет | `closeOnCopy` → `Write → HideShelf → Hud(copied)`; иначе `Write` → `copied/no_target` |
| `keepOpen` + elevated / нет цели | `Write` (шелф виден — тост показывает фронт) |
| без шелфа (хоткей очереди), цель = текущее активное окно | `Write → WaitKeys → FocusTarget (сразу ок) → Settle → Inject → After → HudIfNotPasted`; elevated → `Write → Hud(elevated)`; нет цели → `Write → Hud(copied)` |

Почему так (отклонения от первоначального эскиза «hide → focus → запись»):
- **Write первым**, пока шелф виден: занятый буфер (`failed/clipboard_busy`) обнаруживается до
  того, как что-то сдвинулось, и шелф может показать ошибку. От записи до ввода всё равно ≥ 30 мс.
- **WaitKeys**: ждём (≤ 1 с), пока отпущены Enter, 1–9 (и NumPad 1–9), главная клавиша хоткея
  очереди — иначе автоповтор после смены фокуса попал бы в цель («двойной Enter», «вставилась v»).
  Не отпущены за 1 с → ввод не посылаем (`copied/focus_lost`).
- **FocusTarget до HideShelf**: `SetForegroundWindow(target)`, пока процесс Liqlip — foreground
  (шелф активен), разрешён («The calling process is the foreground process»; плюс «received the last
  input event» — Enter/клик в шелфе) —
  [SetForegroundWindow](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow).
  Шелф прячется уже неактивным — система не активирует «следующее» постороннее окно. Ожидание
  `GetForegroundWindow() == target` шагом 10 мс ≤ 500 мс; на половине — **одна** повторная
  `SetForegroundWindow`. Сознательно **не** используются `AttachThreadInput` к чужому потоку
  (сливает состояние ввода двух процессов), `SPI_SETFOREGROUNDLOCKTIMEOUT` (системная настройка —
  CLAUDE.md, правило 6), инжект Alt для «разблокировки». Свёрнутую цель не разворачиваем.
- **Inject** перепроверяет foreground прямо перед `SendInput`; не совпало → ввода нет, данные в
  буфере, HUD `copied`, `copied/focus_lost`.
- `keepOpen`: шелф не прячется; на время передачи фокуса `focus::handoff_active() == true` —
  выход шелфа по `Focused(false)` (W1-D) обязан его игнорировать; после вставки
  `SetForegroundWindow(шелф)`. Если Windows не вернёт активацию (наш процесс уже не foreground),
  шелф остаётся видимым, но пассивным — клик в него активирует.
- Исход: `pasted`; `copied` + `no_target | elevated | focus_lost`; `failed` + `clipboard_busy |
  not_found | stack_empty`; `partial` (аддитивно в контракт, W1-C) — к `pasted/copied`, если часть
  записей пропущена.

### 6.7. Очередь вставки (`stack.rs`)

`StackState` в памяти процесса (`Vec<StackItem>`, ключи `s1, s2…` уникальны — одна запись может стоять
дважды). `snapshot`/`len` не трогают БД (трей зовёт их из любого потока). Каждая операция сначала
**чистит** элементы, чьи записи удалены (`store.get_for_paste` → `not_found`), и после любой мутации
шлёт `stack:changed` + `tray::refresh`. `stack_add` — все id должны существовать (как в моке),
`stack_move(key, null)` — в конец, неизвестный ключ → `not_found`. `push(id)` — для режима сбора
захвата. `stack_paste_next` / хоткей: верхний элемент → если активен сам шелф (поповер очереди) —
поток шелфа к запомненной цели, иначе — **текущее активное окно** без шелфа (те же проверки
UIPI/фокуса); снимается, если оказался в буфере (`pasted` или `copied`); `stack.newlineAfterPaste` →
Enter после вставки; пустая очередь → `failed/stack_empty` (+ HUD `stack_empty` только на пути
хоткея). `stack_paste_all(separator)` — вся очередь одной мультивставкой (очередь не очищается — для
этого «Очистить»). Вставки из очереди сериализованы: два быстрых нажатия вставят два разных элемента.

### 6.8. Открыть, показать, drag-out (`open.rs`, `dragout.rs`)

- `clip_open`: ссылка → `tauri-plugin-opener` `open_url` (только `http/https/ftp/mailto`; `www.` →
  `https://`; `file:` → локальный путь; `javascript:` и прочие протоколы → `invalid`); файл — первый
  путь, картинка — `media\<id>.png` → `open_path` (нет файла → `not_found`).
- `clip_open_with`: `SHOpenWithDialog` + `OAIF_EXEC` на отдельном STA-потоке (диалог — только для
  одного файла; без `OAIF_EXEC` пользователь получил бы подсказку о настройках) —
  [SHOpenWithDialog](https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shopenwithdialog);
  отмена диалога — не ошибка.
- `clip_reveal`: `opener::reveal_items_in_dir` = `SHOpenFolderAndSelectItems` (+ запасной
  `ShellExecuteEx` плагина) для существующих путей / PNG картинки.
- `clip_drag_out`: перенос Liquid Tools `dragout.rs` без `screenshot::wait_file_ready`. Файлы и
  картинки (их PNG) важнее текста: файлы — shell data object путей (CF_HDROP…) + пути текстом +
  `Preferred DropEffect = COPY`; **одна** картинка — дополнительно `FileGroupDescriptorW`/`FileContents`
  (их читают цели на Chromium/Electron), `PNG`, `CF_DIB`, превью ≤ 192 px как drag-image
  (`IDragSourceHelper::InitializeFromBitmap`); только текст — `CF_UNICODETEXT` + `CF_TEXT` (ANSI через
  `WideCharToMultiByte(CP_ACP)`). Модальный цикл `SHDoDragDrop` на главном потоке (без своего
  изображения Shell рисует общее —
  [SHDoDragDrop](https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shdodragdrop));
  `dropped` = цель приняла `DROPEFFECT_COPY`, иначе `cancelled`. Пока идёт перетаскивание,
  `dragout::active() == true` — шелф не должен закрываться по потере фокуса.

### 6.9. Ограничения

- Нативно не проверено (PQ-02): фокус, UIPI, `SendInput`, drag-out — по чек-листу W1-C на INT-1.
- Приложения, которые сами уводят фокус после активации (редко), дадут `focus_lost` — данные в
  буфере, HUD подсказывает Ctrl+V.
- `keepOpen`: возврат активации шелфу зависит от правил foreground Windows (см. 6.6).
- Внутри окна-цели фокус возвращается в тот элемент, который приложение само восстанавливает при
  активации (как при Alt+Tab).

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
  reason?: 'no_target' | 'elevated' | 'focus_lost' | 'clipboard_busy' | 'not_found' | 'stack_empty' }
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
`"busy: …"`, `"invalid: …"`, `"failed: …"` — сбой ОС/плагина, добавлен в W0; заглушки —
`"unimplemented: <команда>"`); фронт показывает локализованный текст по коду
(`src/lib/ipc/errors.ts` → `common.error.<код>`, неизвестный код → `common.error.unknown`).

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

### 9.9. Уточнения контракта (W0, 2026-11-26)

Аддитивно, без переименований; `model.rs` ≡ `types.ts` проверяет `tests/contract.test.mjs`.

- **Wire-формат.** Структуры — camelCase. Опциональное поле TS (`x?:`) ↔ `Option<T>` с
  `skip_serializing_if` (при `None` ключа нет, а не `null`); `T | null` ↔ `Option<T>` (всегда есть
  ключ). `PasteMode` — внешне-тегированный enum serde: `"auto"`, `"plain"`, `{"format":"html"}`.
  `Record<Kind, T>` ↔ `PerKind<T>` (все 7 ключей). `Counts.folders` — объект с ключами-строками id.
  `ClipDetail` в Rust — `#[serde(flatten)] clip: Clip` + поля деталей.
- **Имена аргументов (ключи JS)** — как в таблице §9.6; для команд с одним объектом:
  `clips_list {q}`, `clip_paste {req}`, `clip_copy {req}` (тип `CopyRequest`), `text_paste {req}`,
  `snippet_save {input}`, `settings_set {settings}`. Остальные: `ids, value, id, beforeId, folderId,
  title, text, token, minutes, scope, name, withClips, key, beforeKey, separator, restoreFocus,
  section, which, mode`. Опциональные (`clip_create_text.folderId`, `settings_open.section`) можно
  не передавать — Tauri даёт `None`.
- **Формы, которые §9.6 не задавал:** `stack_add(ids: number[])`, `stack_remove(key)`,
  `stack_move(key, beforeKey: string|null)`, `stack_get/stack_reverse/stack_clear()` — все → `Stack`;
  `folder_rename(id, name) → Folder`, `folder_move(id, beforeId) → ()`; `snippet_delete`,
  `snippet_mark_used`, `clip_open*`, `clip_reveal`, `picker_hide`, `picker_make_interactive`,
  `settings_open`, `hotkey_record_*`, `open_path`, `ocr_reindex`, `text_copy` → `()` (в JSON `null`);
  все `capture_*` → `CaptureStatus`. `beforeId/beforeKey: null` = в конец.
- **Именованные типы для inline-форм:** `CopyRequest`, `UndoResult {undoToken}`, `ClearResult
  {undoToken|null}`, `ClearScope`, `DragOutcome`, `PathKind`, `BackupMode`, `RunningApp`,
  `MonitorInfo`, `AppMeta` (`app_info`), `OcrStatus`, `ClipFile`, `ClipColor`, `OcrState`,
  `ContactKind`, секции настроек (`GeneralSettings`, `PickerSettings`, `HotkeySettings`,
  `StorageSettings`, `PrivacySettings`, `StackSettings`, `OcrSettings`, `WindowSettings`, `Bounds`,
  `PasteKey`, `PasteChord`, `Retention`, `RememberSearch`, `MergeSeparator`, `Language`, `ThemePref`),
  `SettingsError`, `PasteResult`, `PasteReason`; payload событий: `ClipsChanged`/`ClipsChange`,
  `PickerOpen`/`PickerMode`, `PickerClose` (`{}`), `PickerLayout` (ширина/высота панели **без** bleed и
  bleed — логические px), `SettingsNavigate`, `HotkeysCaptured` (**`combo: ""` = запись отменена**, Esc),
  `HudShow`/`HudKind`, `OcrProgress`, `ImportProgress`, `ResolvedTheme`.
- **`PasteReason` + `stack_empty`** (координатор, приёмка W0): `stack_paste_next`/`stack_paste_all` при пустой очереди → `{ result: 'failed', reason: 'stack_empty' }` (хоткей дополнительно показывает `hud::show(StackEmpty)`).
- **Безопасные заглушки** (до W1 возвращают `Ok`): `capture_status` (запись идёт), `stack_get`
  (пусто), `picker_target` (`null`). **Реализованы в W0:** `settings_get`, `settings_set`, `app_info`,
  `open_path`, `pick_exe`. Остальные 57 → `Err("unimplemented: <имя>")`.
- **`settings_set`**: семантически неверные поля сохраняют прежнее значение и попадают в `errors`
  (`invalid`); типовой мусор (не тот JSON-тип, неизвестное значение enum) отклоняется Tauri до вызова
  (ошибка команды). Хоткеи — синхронный хук `hotkeys::apply_settings` (§15): отвергнутое поле
  возвращается к прежнему. `autostart` применяется при изменении (в любой сборке) и сверяется на
  старте (только release — debug-сборка не прописывает dev-бинарник в автозапуск). Успешное
  изменение: атомарная запись `settings.json` → подписчики → `settings:changed` ⚑ → при смене темы
  `theme://changed` ⚑ (разрешённая тема). Загрузка `settings.json` — по полям: неверное поле → дефолт.
- **Тема.** `theme://changed` несёт разрешённую тему (`'light'|'dark'`); шлётся при смене
  `general.theme` и при смене темы Windows, если выбрано `system`.

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

**Перенос W0 (2026-11-26).** `app.css` — токены 1:1 (палитры обеих тем, радиусы, тени, шкала,
моушен, `squircle`, `.ico`, `pop-select`, reduced-motion); выкинуты только `.studio-shot-corners`,
`--gradient-avatar` (аккаунт), `--dur-cap-morph` (колпачок диктовки), `body.liquid-main` (лаунчер).
Факт исходника: в **светлой** теме Liquid Tools `hairline`/`hairline-strong`/`divider` не прозрачны
(`#dededc`/`#b7b7b4`/`#e4e4e2`; прозрачны только в тёмной) — перенесено как есть, компоненты эти
токены под границы не используют (правило «без границ» в силе); единственное употребление — цвет
тонкого скроллбара. Компоненты `src/lib/components/` — копии Liquid Tools (`Ico`, `Button`, `Toggle`,
`SegmentedControl`, `Dropdown` (bits-ui), `MenuItem`, `Skeleton`, `EmptyState`, `Toast`, `FlashPill`,
`Kbd`, `Badge`, `SearchField`, `BigTypeList`); `Menu` очищен от вкладок/аккаунта/перетаскивания окна
(просто список `MenuItem` для сайдбара); захардкоженные `aria-label` (`Dismiss`, `Clear`) заменены
ключами `common.*`. Иконки — 75 Heroicons outline (`@iconify-json/heroicons` 1.2.3, формат Liquid
Tools: stroke 1.5, `1em`), реестр `src/lib/icons.ts` (76 имён), лицензия `public/licenses/heroicons.txt`.

## 12. Проверка (гейты)

| Гейт | Команда |
|---|---|
| Типы Svelte/TS | `pnpm check` → 0 ошибок, 0 предупреждений |
| Node-тесты | `pnpm test` (`node --test "tests/*.test.mjs"`) |
| Сборка фронта | `pnpm build` (единственное, что резолвит импорты иконок) |
| Rust | `cargo check --manifest-path src-tauri/Cargo.toml` (0 предупреждений); `cargo test --manifest-path src-tauri/Cargo.toml` — **работает** (W0) |
| UI в браузере | `pnpm dev` (порт **1430**, strict — не конфликтует с Liquid Tools 1420) + мок-адаптер |
| Нативно | `pnpm tauri dev` — агенты волн **не запускают**; координатор запускает один раз на интеграции (INT-*) для сквозных сценариев (решение Eddy 2026-11-26, PQ-02); живая проверка — Eddy по установщику (было: правило воркспейса — агенты нативное приложение не запускают, пока Eddy не разрешит) |

**Итог W0 по тестам (2026-11-26).** `cargo test` в крейте `src-tauri` запускается и проходит
(24 теста: контракт serde, хэш, settings, i18n_native, трей, logx/fsx, FTS5-зонд) — тест-бинарник
линкуется со всеми плагинами (`dialog`, `autostart`, `opener`, `single-instance`) без
`STATUS_ENTRYPOINT_NOT_FOUND`. Значит, **Rust TDD доступен прямо в модулях** (`#[cfg(test)]` рядом с
кодом); отдельный крейт `crates/liqlip-core` не создавался. Если новая зависимость W1 вернёт падение
тест-бинарника (импорт из Common Controls v6 без манифеста — tauri-build встраивает манифест только в
`bins`), первое средство — встроить тот же манифест и в тесты из `build.rs`
(`cargo:rustc-link-arg-tests=/MANIFEST:EMBED` + `/MANIFESTINPUT:<файл>`), и только потом выносить
логику в отдельный крейт; сообщить координатору.

`pnpm test` = `node --test "tests/*.test.mjs"`: на Node 24 аргумент-каталог (`node --test tests/`)
трактуется как путь к модулю и падает, поэтому — glob. Тесты W0: `contract` (§9.6 ≡ `lib.rs` ≡
`commands.ts` ≡ мок; `model.rs` ≡ `types.ts` поле в поле, включая опциональность, и enum ≡ union),
`settingsDefaults` (мок ≡ `tests/fixtures/settings.default.json` ≡ Rust), `ipcMock` (поведение мока),
`hotkeyCapture` (перенос). Хелпер `tests/helpers/bundle.mjs` собирает TS-модули esbuild'ом в один ESM.

## 13. Владение файлами и правила для агентов

- **Файлы-узлы координатора:** `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`,
  `src-tauri/capabilities/*.json`, `src-tauri/src/model.rs` + `src/lib/ipc/types.ts` (контракт),
  `src/lib/i18n/{en,ru}.ts` (агенты дописывают только свой префикс), `src/app.css`.
  С W0 к ним относятся и общие модули: `src-tauri/src/{settings,content,events,tray,theme,
  i18n_native,logx,fsx,autostart}.rs`, `src/lib/ipc/{commands,events,runtime,errors}.ts`,
  `src/lib/ipc/mock/bus.ts`, `src/lib/{appearance,theme.svelte,flash.svelte,hotkey,hotkeyCapture,
  hotkeyError}.ts`, `src/lib/components/**`, `tests/{contract,settingsDefaults,ipcMock}.test.mjs`,
  `tests/helpers/`, `tests/fixtures/`. Нужна правка — сниппет в отчёте.
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

## 15. Межмодульные API (W0, 2026-11-26)

Контракт **между Rust-модулями** — то, против чего агенты W1 пишут параллельно. Все функции
объявлены W0 с этими сигнатурами и безопасными заглушками (без `todo!()`/паник): «чтение» отдаёт
пустое/дефолт, «действие» — `Err("unimplemented: <модуль>::<функция>")`. Владелец модуля наполняет
тело; **сигнатуры меняет только координатор** (аддитивно). Нужно другое — в отчёт, временно
обойти у себя.

**Правила.**
1. **`AppHandle` — единственный путь к общему состоянию и событиям.** Managed state достаётся через
   `app.state::<T>()` (`Store`, `settings::SettingsStore`); модули с потоками получают `AppHandle` в
   своём `init/start` (setup §3) и хранят клон. Глобальные статики — только внутреннее дело модуля
   (исключение по контракту — `capture::mark_own_write`, вызываемый без `AppHandle`).
2. **События — только через `events.rs`** (`events::clips_changed(app, &ClipsChanged{…})` и т. д.):
   имена и адресация §9.7 в одном месте. Строковые литералы событий в модулях запрещены.
3. **`Store` не эмитит** — эмитит вызывающий, он знает `origin` (`capture` / `user` / `import`).
   Фоновые задачи store (ретенция, чистка) эмитят сами (`origin: 'retention'`).
4. Всё **блокирующее**: из команд — `tauri::async_runtime::spawn_blocking`/свой worker; из UI-потока
   и из коллбека LL-хука — никогда.
5. Ошибки — `Result<_, String>` с кодом `not_found | busy | invalid | failed`.
6. После изменения того, что показывает трей (пауза, длина очереди), вызвать `tray::refresh(app)`.

**Координатор (реализовано в W0)**

```rust
// content.rs — ЕДИНСТВЕННЫЙ алгоритм хэша (§4.2) и правила содержимого (SPEC §2.3)
pub fn content_hash(kind: Kind, canonical: &[u8]) -> String;        // 32 hex, XXH3-128
pub fn image_hash(width: u32, height: u32, rgba: &[u8]) -> String;  // = content_hash(Image, w‖h‖rgba)
pub fn canonical_text(text: &str) -> &[u8];
pub fn canonical_files(paths: &[String]) -> Vec<u8>;                 // lower, sort, join "\n"
pub fn truncate_text(text: &str) -> (&str, bool);                    // ≤ TEXT_MAX_BYTES (2 МБ)
pub fn make_preview(text: &str) -> String;                           // ≤ 512 символов
pub fn text_metrics(text: &str) -> TextMetrics;                      // { chars, words, lines }
pub const PREVIEW_MAX_CHARS, TEXT_MAX_BYTES, FILES_MAX /*2000*/, IMAGE_MAX_PIXELS /*64 МП*/;

// settings.rs
pub fn get(app: &AppHandle) -> Settings;                             // клон, только read-lock
pub fn subscribe(app: &AppHandle, f: impl Fn(&Settings /*old*/, &Settings /*new*/) + Send + Sync + 'static);
//   вызывается после сохранения, до settings:changed; НЕ вызывать settings_set из подписчика
pub fn patch_window_bounds(app: &AppHandle, bounds: Bounds);         // без рассылки

// events.rs — по функции на событие §9.7
pub fn clips_changed(app, &ClipsChanged);  pub fn picker_open(app, &PickerOpen);
pub fn picker_close(app);                  pub fn picker_layout(app, &PickerLayout);
pub fn capture_status(app, &CaptureStatus); pub fn stack_changed(app, &Stack);
pub fn settings_changed(app, &Settings);   pub fn settings_navigate(app, section: &str);
pub fn hotkeys_captured(app, combo: &str); pub fn hud_show(app, &HudShow);
pub fn ocr_progress(app, &OcrProgress);    pub fn import_progress(app, &ImportProgress);
pub fn theme_changed(app, ResolvedTheme);
pub const PICKER: &str = "picker"; SETTINGS = "settings"; HUD = "hud";   // метки окон

// tray.rs
pub fn refresh(app: &AppHandle);           // перестроить меню/иконку/подсказку (любой поток)
// theme.rs: apply(app, ThemePref) · resolved(app, ThemePref) -> ResolvedTheme · emit_changed(app, ThemePref)
// i18n_native.rs: resolve(Language) -> Lang · tr(Lang, Key) -> &'static str  (строки трея/диалогов)
```

**store (W1-A)** — `src-tauri/src/store/mod.rs`

```rust
pub struct Store { /* W1-A */ }                         // managed state
impl Store {
    pub fn open(data_dir: &Path) -> Result<Store, String>;                  // setup §3 шаг 4
    pub fn insert_clip(&self, clip: NewClip) -> Result<InsertOutcome, String>; // ← capture
    pub fn set_app_icon(&self, app_id: i64, png: &[u8]) -> Result<(), String>; // ← capture
    pub fn get_for_paste(&self, id: i64) -> Result<PastePayload, String>;      // ← paste
    pub fn mark_used(&self, ids: &[i64], bump: bool) -> Result<(), String>;    // ← paste (bump = picker.bumpOnPaste)
    pub fn delete_recent(&self, minutes: i64) -> Result<i64, String>;          // ← трей, clips_delete_recent
}
pub fn init(app: &AppHandle);                                // фон: ретенция, чистка, subscribe(storage.*)
pub fn serve_uri(app: &AppHandle, path: &str) -> Option<(String /*mime*/, Vec<u8>)>; // схема liq, §9.8

pub struct NewClip {            // всё уже посчитано захватом
    kind: Kind, hash: String /*content::*hash*/, preview: String /*content::make_preview*/,
    text: Option<String> /*текстовые виды; для file — пути через \n; None для image*/,
    meta: ClipMeta /*в т.ч. truncated*/, size: i64 /*только свои байты*/,
    char_count, word_count, line_count: Option<i64> /*content::text_metrics*/,
    width, height, file_count: Option<i64>,
    png: Option<Vec<u8>> /*image — сама запись; текст — PNG-альтернатива (Excel)*/,
    thumb_png: Option<Vec<u8>> /*≤360 px, только image*/,
    html: Option<String> /*CF_HTML без заголовка: StartHTML..EndHTML, UTF-8*/,
    rtf: Option<Vec<u8>>, files: Option<ClipFiles>, source: Option<SourceApp>, copied_at: i64,
}
pub struct ClipFiles { paths: Vec<String> /*≤ FILES_MAX*/, drop_effect: u32 /*DROPEFFECT_*, по умолчанию 1*/ }
pub struct SourceApp { exe: String /*полный путь, lower*/, name: String /*FileDescription|имя файла*/ }
pub struct InsertOutcome { id: i64, created: bool /*false = поднята существующая*/, app_id: Option<i64>,
                           app_icon_missing: bool /*→ захват извлекает иконку один раз*/ }
pub struct PastePayload { id: i64, kind: Kind, text: Option<String>, html: Option<String>,
                          rtf: Option<Vec<u8>>, png_path: Option<PathBuf>, files: Option<ClipFiles>,
                          formats: Vec<Format> }
```

Метрики и хэш считает **захват** (и store — для `clip_update_text`/`clip_create_text`) одними и теми
же функциями `content.rs`; классификацию для `clip_create_text` store берёт у `capture::classify`.

**capture (W1-B)** — `src-tauri/src/capture/{mod,classify}.rs`

```rust
pub fn start(app: &AppHandle);                                    // setup шаг 5: listener + worker
pub fn mark_own_write(seq: u32);                                  // ← paste: GetClipboardSequenceNumber после записи
pub fn status(app: &AppHandle) -> CaptureStatus;                  // ← трей, capture_status
pub fn pause(app: &AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String>; // ← трей
pub fn resume(app: &AppHandle) -> Result<CaptureStatus, String>;  // ← трей
pub fn toggle_pause(app: &AppHandle) -> Result<CaptureStatus, String>; // ← хоткей PauseToggle
pub fn ignore_next(app: &AppHandle) -> Result<CaptureStatus, String>;  // ← трей, хоткей IgnoreNext
pub fn app_icon_data_url(exe: &str) -> Option<String>;           // ← paste (TargetInfo.iconUrl), apps_running
pub mod classify { pub fn classify_text(text: &str) -> (Kind, ClipMeta); }   // ← store
```
Захват после `insert_clip` эмитит `clips:changed { origin: 'capture', change: created ? 'added' :
'updated', ids }`, в режиме сбора зовёт `paste::stack::push`, при смене состояния —
`events::capture_status` + `tray::refresh`.

**paste (W1-C)** — `src-tauri/src/paste/{mod,focus,stack}.rs`

```rust
pub fn init(app: &AppHandle);                                         // setup шаг 5
pub mod focus {
    pub fn capture_target(app: &AppHandle) -> Option<TargetInfo>;     // ← picker::summon ДО показа
    pub fn current_target(app: &AppHandle) -> Option<TargetInfo>;     // ← picker_target, подсказки
}
pub mod stack {
    pub fn push(app: &AppHandle, clip_id: i64) -> Result<(), String>; // ← capture (режим сбора)
    pub fn snapshot(app: &AppHandle) -> Stack;                        // ← stack_get
    pub fn len(app: &AppHandle) -> usize;                             // ← трей «Очередь: N»
    pub fn paste_next(app: &AppHandle) -> Result<PasteOutcome, String>; // ← хоткей StackNext (на worker'е)
}
```
Поток вставки: `picker::hide_now` → фокус цели → `store.get_for_paste` → запись в буфер →
`capture::mark_own_write(seq)` → ввод → `store.mark_used` → `clips:changed {origin:'user'}`; нет
фокуса/elevated → `picker::hud::show(…)`.

**picker + hud (W1-D)** — `src-tauri/src/picker/{mod,hud}.rs`

```rust
pub fn init(app: &AppHandle);                          // после создания окон: subscribe(picker.*), hover-поллер
pub fn summon(app: &AppHandle);                        // ← хоткей, трей, single-instance (повтор = закрыть)
pub fn hide_now(app: &AppHandle);                      // ← paste: мгновенно, БЕЗ возврата фокуса
pub fn show_settings(app: &AppHandle, section: Option<&str>); // ← settings_open, трей, онбординг (setup шаг 10)
pub mod hud { pub fn show(app: &AppHandle, kind: HudKind, text: Option<String>); } // ← paste, хоткеи
```

**hotkeys (W1-D)** — `src-tauri/src/hotkeys/mod.rs`

```rust
pub enum HotkeyAction { Summon, StackNext, PauseToggle, IgnoreNext }
pub fn start(app: &AppHandle, hotkeys: &HotkeySettings) -> Vec<SettingsError>;   // setup шаг 7
pub fn apply_settings(app: &AppHandle, prev: &HotkeySettings, next: &HotkeySettings) -> Vec<SettingsError>;
//   ← settings_set (синхронно); field "hotkeys.<имя>", code taken|invalid; движок держит прежнюю привязку
```
Действия: `Summon → picker::summon`, `StackNext → paste::stack::paste_next` (не в коллбеке хука),
`PauseToggle → capture::toggle_pause`, `IgnoreNext → capture::ignore_next`.

**ocr (W2-A)** — `pub fn start(app: &AppHandle)` (setup шаг 5) + команды. **import (W2-B)** — только
команды.

**Фронт.** Окна общаются с Rust только через `src/lib/ipc/commands.ts` (`call` → `invoke` или мок) и
`events.ts` (`on*` → `listen` или мок-шина). Тема/язык окна — `initAppearance()` из `appearance.ts`
(`settings_get` + `settings:changed`, плюс `theme://changed`). Общая развёртка плейсхолдеров —
`src/lib/snippets/` (W1-F). Мок: `mock/index.ts` (роутер, `MOCK_COMMANDS`), `bus.ts` (шина событий,
флаги `?mock-empty=1`, `?mock-error=1`, `?mock-live=0`, `?mock-size=N`,
`?mock-target=chrome|none|elevated`, `?mock-unimpl=ocr,import`; задержка 30–80 мс), стабильный API
`settings.ts::currentSettings()` для остальных мок-модулей.
