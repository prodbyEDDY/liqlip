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
| Rust | `rusqlite` **0.43.0** `bundled` (libsqlite3-sys 0.40.0 → SQLite **3.54.1**, собрано с `SQLITE_ENABLE_FTS5`; на rustc 1.99 собирается — пин `=0.38` из Liquid Tools не нужен), `windows` 0.62.2 (Win32/WinRT; фичи — по назначению, с комментарием `# W1-X`), `image` 0.25.10 (`png`, `bmp`; W6-A: `jpeg` — файлы снимков в JPG, в лок добавлены только `zune-core` 0.5.3 и `zune-jpeg` 0.5.15), `xxhash-rust` 0.8.21 (`xxh3`, общий хэш — §15), `serde` 1.0.228, `serde_json` 1.0.150, `parking_lot` 0.12.5, `base64` 0.22, `tokio` 1.52 (`sync`). `arboard` W0 **не** добавлял — чтение/запись своими Win32-вызовами (решают W1-B/W1-C) | `enigo` **не** используем — `SendInput` напрямую |
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
  capture.html · shots.html · editor.html · pin.html   окна скриншотов (W5, §18)
  src/
    app.css                     токены (перенос из Liquid Tools, монохром-лок)
    lib/
      components/               общие компоненты (Ico, Button, Toggle, SegmentedControl, Dropdown, Skeleton,
                                EmptyState, Toast, Kbd, Badge, SearchField; неиспользуемые удалены в W3-D)
      heroicons/ · icons.ts     иконки
      i18n/                     en.ts (источник ключей) · ru.ts (Record<TranslationKey,string>) · index.svelte.ts ·
                                format.ts (подстановка {слотов} за один проход, тест i18n)
      ipc/
        types.ts                ← КОНТРАКТ (§9), зеркало src-tauri/src/model.rs
        commands.ts             типизированные обёртки invoke (одна функция на команду)
        events.ts               типизированные подписки
        runtime.ts · errors.ts  isTauri() · код ошибки → локализованный текст (common.error.*)
        frame.ts                кадрирование сырого тела коммита скриншота (W5, §18.4)
        mock/                   браузерный адаптер: index (роутер) · bus (события, флаги URL, задержка) ·
                                seed · clips (store+paste+stack+симулятор захвата) · settings · system ·
                                shots (скриншоты, W5)
      motion/ · theme.svelte.ts · appearance.ts · hotkey.ts · hotkeyCapture.ts · scrollFade.ts (затухание краёв
                                прокрутки, W3-D) · snippets/expand.ts (развёртка плейсхолдеров) ·
                                browserKeys.ts (клавиши браузера WebView2, которые глотает каждое окно, W4)
    windows/
      picker/                   шелф (Picker.svelte + компоненты, cards/, layout.ts, query.ts, transforms.ts …)
      settings/                 окно настроек + онбординг
      hud/                      HUD-тост
      capture/ · shots/ · editor/ · pin/   окна скриншотов (W5 — заглушки корней; W6-C/D/E, §18)
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
      screenshot/               (W5–W6) скриншоты: команды-делегаты, сессия, коммит, окна (§18)
      content.rs                общий хэш + правила содержимого (превью, усечение, метрики) — §15
      events.rs                 типизированные эмиттеры всех событий §9.7 (имена + адресация)
      tray.rs · theme.rs · logx.rs · fsx.rs · autostart.rs · i18n_native.rs
    tauri.conf.json · capabilities/{default,screenshot}.json · icons/ (знак Liqlip, W2-D — §16)
  tests/                        *.test.mjs (node:test) · helpers/bundle.mjs (esbuild → ESM) ·
                                fixtures/settings.default.json (дефолты: Rust ≡ мок)
  docs/                         эта библиотека
```

## 2. Окна

Все четыре окна (с W5 — и оверлей `capture`) **создаются заранее скрытыми в `setup()`** и только показываются (правило Liquid
Tools: ленивый `.build()` из команды/хоткея дедлочит event loop на Windows).

| Окно | Свойства | Кто показывает |
|---|---|---|
| `picker` | прозрачное, без рамки, always-on-top, skip-taskbar, не в Alt+Tab (`WS_EX_TOOLWINDOW`), `disable_drag_drop_handler`, размер по расположению + прозрачное поле `BLEED` под тень | хоткей, трей, наведение на край |
| `settings` | обычное окно Windows: системный заголовок, `decorations: true`, мин. 720×520, запоминает размер/позицию (`settings.json` → `window`), **CloseRequested → hide**, а не уничтожение | трей, `Ctrl+,` в шелфе, онбординг при первом запуске |
| `hud` | прозрачное, без рамки, always-on-top, skip-taskbar, **неактивируемое** (`WS_EX_NOACTIVATE`), click-through, ~320×56 | Rust: `hud::show(kind, text)` у курсора, автоскрытие 1.6 с |
| `capture` (W5) | оверлей захвата: прозрачное, без рамки, always-on-top, skip-taskbar, focusable, `shadow(false)`, `disable_drag_drop_handler`, `WDA_EXCLUDEFROMCAPTURE`; заранее размер основного монитора | сессия скриншота (W6-A, §18.3) |

У всех окон — `CloseRequested` перехвачен (Alt+F4 не уничтожает заранее созданные окна;
в Liquid Tools это баг шелфа). Окна скриншотов `shots`, `editor`, `pin-<n>` строятся **по требованию**
(единственное исключение из правила, §18.2).

**`capture` в W5** (`lib.rs::build_windows`, после `settings`): параметры выше, `inner_size` 800×600 →
сразу позиция → размер → позиция = основной монитор (`primary_monitor()`, физ. px; частый случай
«один монитор» — голый `show`). `screenshot::windows::exclude_from_capture` —
`SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` (learn.microsoft.com: окно видно на мониторе,
но не попадает ни в какой захват; с Windows 10 2004, раньше Windows применяет `WDA_MONITOR`; только
при композиции DWM; фичи `windows` не добавлялись — `Win32_UI_WindowsAndMessaging` уже есть).
`CloseRequested` → `prevent_close` + `screenshot::on_capture_close` (отмена сессии, W6-A) + `hide`.
Ошибка построения окна не прерывает запуск: предупреждение в лог, скриншоты недоступны.

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
5. `capture::start` (listener-поток) · `paste::init` · `ocr::start` (W2) · W5:
   `manage(screenshot::ScreenshotState)` + `screenshot::init` (до окон: оверлей может спросить
   `shot_session` сразу при загрузке).
6. Окна `picker`, `hud`, `settings`, W5: `capture` (скрытые).
7. `hotkeys::start` + применение `settings.hotkeys`.
8. `tray::build` (подписи из `i18n_native` по языку).
9. `theme::apply`.
10. Если `!settings.general.onboardingDone` → показать `settings` в режиме онбординга.

Реализовано в W0 ровно в этом порядке (`lib.rs::run`); там, где модуль ещё заглушка, вызывается
заглушка §15 (`capture::start`, `paste::init`, `ocr::start`, `picker::init`, `hotkeys::start`,
`picker::show_settings(Some("onboarding"))`). `Store::open` → `Err` прерывает запуск (W1-A делает
открытие устойчивым: битая база отодвигается и создаётся заново). На `RunEvent::Exit` сохраняется
геометрия окна настроек.

**Главный поток не ждёт замок настроек** (W3-A). `settings_set` держит `SettingsStore.write`, пока
работают подписчики, а часть из них ждёт главный поток (геттеры окон Tauri — круговой рейс без
таймаута). Поэтому `lib.rs::save_settings_bounds` читает геометрию на главном потоке, а
`patch_window_bounds` зовёт на коротком потоке: при закрытии окна — не ждёт, на `RunEvent::Exit` —
ждёт ≤ 1 с. Раньше смена языка/темы одновременно с закрытием окна настроек могла намертво повесить
приложение. Восстановление геометрии — позиция → размер → позиция (как докинг шелфа §7.4): размер,
выставленный до переезда на монитор с другим масштабом, tao пересчитал бы по `WM_DPICHANGED`.

## 4. Данные

> Замер §4: 2026-11-26 (W1-A: хранилище реализовано целиком — `src-tauri/src/store/`;
> `cargo test` 76/76, из них 53 теста хранилища, включая проверку планов запросов; нативно не
> запускалось, стенда на 50 000 записей нет — решение Eddy). W3-B (2026-11-26): ревью данных и
> исправления — удаления выбирают строки внутри своей транзакции, ретенция щадит записи под undo и
> ждёт настроек восстановленной копии, приватные действия чистят старые копии данных, `-"фраза"`,
> спецсимволы и длинные термы в поиске, `ClipMeta.dir` (FIX-08), восстановление прерванного
> `replace`, уведомления настроек вне замка записи (находка W3-A); `cargo test` 334/334 дважды (после слияния W3-A).
> W6-A (2026-11-27): схема v2 (`shot_docs`, §4.2a), вид `screenshot` во всех ветках хранилища
> (вставка, лента, превью, OCR, сироты, бэкап), `media\base\`; `cargo test` 394/394 дважды; нативно
> не запускалось.

### 4.1. Файлы

```
%APPDATA%\com.liqlip.app\
  liqlip.db (+ -wal, -shm)     SQLite (WAL)
  liqlip.db.check              маркер: в работе встретился SQLITE_CORRUPT → на следующем старте quick_check
  liqlip.db.corrupt-<ms>       битая база, отодвинутая при открытии (+ media.corrupt-<ms>, icons.corrupt-<ms>)
  media\<id>.png               полноразмерные картинки и PNG-альтернатива текстовых записей (Excel)
  media\thumbs\<id>.png        превью ≤ 360 px по длинной стороне (image и screenshot)
  media\base\<id>.png          W6-A: картинка снимка / отредактированной картинки без аннотаций (§18.5)
  media\.staging-*.png         временные файлы вставки (переименовываются в <id>.png в транзакции)
  icons\<appId>.png            иконки приложений-источников (одна на exe)
  settings.json                Settings (§9.4), с полем version
  logs\liqlip.log              без содержимого буфера
```

Имена файлов выводятся **только из id**, а id не переиспользуются (`AUTOINCREMENT`), поэтому файл
можно удалять после коммита удаления строки, не боясь задеть новую запись. Пути в
`clip_formats.path` / `apps.icon_path` (`media\<id>.png`, `icons\<id>.png`) — справочные.

### 4.2. Схема SQLite v1 (`PRAGMA user_version = 1`)

Новое приложение — схема своя; после релиза v1 она меняется **только миграциями** (шаг на
версию, каждый в своей транзакции, `store/schema.rs::migrate`). Авторитетная копия SQL —
`store/schema.rs::V1`; здесь — по существу.

**Соединение** (`schema::open_db`, на каждое открытие): `busy_timeout = 3000` →
`auto_vacuum = INCREMENTAL` (только у пустой базы — до первой таблицы) → `journal_mode = WAL` →
`synchronous = NORMAL` · `foreign_keys = ON` · `secure_delete = FAST` ·
`journal_size_limit = 16 МБ` → функция `liq_icontains` → (маркер `.check` → `quick_check`) →
миграции → ремонт инварианта `fav_order` → `PRAGMA optimize = 0x10002`. Одно соединение под
`parking_lot::Mutex` (без отравления); команды работают через `spawn_blocking`.

```sql
CREATE TABLE apps (id INTEGER PRIMARY KEY AUTOINCREMENT, exe TEXT NOT NULL UNIQUE /*lower*/,
                   name TEXT NOT NULL, icon_path TEXT);
CREATE TABLE folders (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                      position REAL NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE clips (
  id INTEGER PRIMARY KEY AUTOINCREMENT,  -- id не переиспользуются: очередь вставки, кэш webview, файлы
  kind TEXT NOT NULL, title TEXT, preview TEXT NOT NULL, text TEXT,
  hash TEXT NOT NULL UNIQUE,             -- content::content_hash / image_hash (§15)
  size INTEGER NOT NULL DEFAULT 0,       -- text + все clip_formats (вкл. полный PNG); превью не в счёт
  char_count, word_count, line_count, width, height, file_count INTEGER,
  app_id INTEGER REFERENCES apps(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL, copied_at INTEGER NOT NULL, copy_count INTEGER NOT NULL DEFAULT 1,
  used_at INTEGER, use_count INTEGER NOT NULL DEFAULT 0,
  favorite INTEGER NOT NULL DEFAULT 0, fav_order REAL,
  folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL,
  deleted_at INTEGER, ocr_text TEXT, ocr_state INTEGER NOT NULL DEFAULT 0, meta TEXT /*ClipMeta JSON*/
);
CREATE INDEX idx_clips_feed     ON clips(copied_at DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_clips_favfirst ON clips(favorite DESC, copied_at DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_clips_kind     ON clips(kind, copied_at DESC, id DESC, deleted_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_clips_folder   ON clips(folder_id, copied_at DESC, id DESC);   -- полный: и для FK
CREATE INDEX idx_clips_app      ON clips(app_id, copied_at DESC, id DESC);      -- полный: и для FK
CREATE INDEX idx_clips_history  ON clips(copied_at, id, size, favorite, folder_id, deleted_at)
  WHERE deleted_at IS NULL AND favorite = 0 AND folder_id IS NULL;              -- ретенция (INDEXED BY)
CREATE INDEX idx_clips_deleted  ON clips(deleted_at, copied_at) WHERE deleted_at IS NOT NULL;
CREATE TABLE clip_formats (clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  format TEXT NOT NULL /*html|rtf|png|files*/, data BLOB, path TEXT, size INTEGER NOT NULL,
  PRIMARY KEY (clip_id, format));
CREATE TABLE snippets (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, body TEXT NOT NULL,
  keyword TEXT UNIQUE, position REAL NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  use_count INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_snippets_position ON snippets(position, id);
CREATE VIRTUAL TABLE clips_fts USING fts5(title, text, ocr_text, content='clips', content_rowid='id',
  tokenize='trigram');
INSERT INTO clips_fts(clips_fts, rank) VALUES ('secure-delete', 1);
-- триггеры clips_fts_ai (AFTER INSERT), clips_fts_ad (AFTER DELETE → 'delete' со старыми значениями),
-- clips_fts_au (AFTER UPDATE OF title, text, ocr_text → 'delete' + insert): подъём copied_at,
-- счётчики и флаги текст не перетокенизируют (lang_createtrigger: UPDATE OF срабатывает, только
-- если колонка есть в SET)
CREATE VIRTUAL TABLE snippets_fts USING fts5(name, body, content='snippets', content_rowid='id',
  tokenize='trigram');   -- + 'secure-delete' + те же три триггера по name, body
```

Отличия от плана W0: `AUTOINCREMENT` на всех таблицах; индекс `idx_clips_fav` не нужен
(планировщик берёт `idx_clips_favfirst (favorite=?)` + маленькую сортировку); добавлены
`idx_clips_favfirst`, `idx_clips_history`, `idx_clips_deleted`, `idx_snippets_position`;
`idx_clips_kind` несёт `deleted_at` хвостом (покрывает счётчики — планировщик не считает условие
частичного индекса выполненным для покрытия); `idx_clips_folder`/`idx_clips_app` с `id` и полные
(обслуживают и `ON DELETE SET NULL`). `clip_formats` 'files' — JSON `{dropEffect, paths}`; 'png' есть
у каждой картинки (единый список форматов и путь для вставки).

**Хэш** (W0, `src-tauri/src/content.rs`, §15): XXH3-128 (`xxhash-rust`), 32 hex-символа, от
`"liqlip:1\0" ‖ семейство ‖ "\0" ‖ каноническое содержимое`. Семейство, а не точный `kind`
(`text` для text/link/code/color/contact, `image`, `file`) — чтобы улучшенный классификатор в будущей
версии поднимал старую запись, а не плодил двойник. Канон: текст — как есть (после усечения до 2 МБ);
картинка — `width u32 LE ‖ height u32 LE ‖ RGBA8` (пиксели, а не байты PNG — не зависит от версии
энкодера); файлы — пути в нижнем регистре, отсортированы, через `\n`. Значение закреплено тестом
`content::tests::hash_is_stable` — менять формат после релиза нельзя. Хранилище своего хэша не
заводит: при усечении текста > 2 МБ / списка > 2 000 путей и при правке/создании текста оно зовёт
те же функции `content.rs`.

**Правила (как реализовано):**
- **Вставка из захвата** (`write.rs::insert_clip`, одна транзакция `IMMEDIATE`): `apps` upsert по exe
  (`exe` хранится в нижнем регистре, `/` → `\`; непустое имя из захвата обновляет старое, пустое —
  оставляет; новое без имени — имя файла без `.exe`). Сопоставление (`write.rs::match_app`, FIX-02,
  W2-F; то же в импорте — `bulk.rs::resolve_app`): тот же exe (индексный поиск) → полный путь при
  строке только с именем файла (импорт Liquid Tools знал лишь `<stem>.exe`) — эта строка получает
  полный путь, имя и иконка сохраняются → голое имя при строке с полным путём того же файла — берётся
  она; два разных полных пути с одним именем файла — два приложения; запись с тем же `hash` (в том числе мягко
  удалённая) → `copied_at = max(copied_at, new)`, `copy_count + 1`, `deleted_at = NULL`, источник —
  последний, `kind` — новая классификация, `meta` — слияние полей (новое побеждает, где оно есть);
  богатые форматы новой копии заменяют одноимённые старые; файлы картинки дописываются, только если
  их нет. Иначе новая строка. PNG и превью пишутся **до** захвата блокировки во временный файл
  `media\.staging-*` и в транзакции переименовываются в `<id>.png` (большая картинка не держит БД);
  сбой → откат + удаление записанного. `size` пересчитывается из того, что реально сохранено.
  Валидация: пустой текст / картинка без валидного PNG / файлы без путей / пустой хэш → `invalid`;
  картинка > 64 МП → `invalid`; html/rtf > 2 МБ — отбрасываются (запись сохраняется); размеры
  картинки берутся из заголовка PNG. После вставки — сигнал фоновому потоку (ретенция с порогом).
  У файловой записи `meta.dir` = каталог первого пути (`content::file_dir`, FIX-08, W3: только строки,
  без обращения к диску; `C:\a.txt` → `C:\`, `\\srv\share\a.txt` → `\\srv\share`) — его хвост показывает
  карточка; то же правило в импорте (`bulk.rs::normalize`), старые записи дозаливает фоновый поток.
- **Мягкое удаление** (`clips_delete`, всё или ничего): `deleted_at = now`; все листинги, `liq`-схема,
  `get_for_paste`, детали фильтруют `deleted_at IS NULL`; фоновая чистка удаляет строки старше
  2 минут жёстко вместе с файлами.
- **`undoToken`** — непрозрачная строка-ключ (`u<seq>.<ms>`); данные (id, тип, прежние значения)
  в памяти, TTL 2 мин, токен срабатывает один раз. Операции: удаление и «Очистить историю»
  (возврат строк, ещё помеченных **этим** `deleted_at`), перенос в папку (прежняя папка каждой
  записи; удалённая с тех пор папка → без папки), удаление папки (строка папки с тем же id; если имя
  занято — «Имя 2»; участники возвращаются в неё, при `withClips` — и из удаления). Событие отмены:
  удаление → `added`, папка → `updated`, очистка → `reset`.
- **Жёсткое удаление** (W3): каждая пачка (≤ 500) **выбирается внутри своей транзакции удаления**
  (`lifecycle.rs::delete_picked`) — ретенция, чистка мягко удалённых и «удалить недавнее». Раньше id
  выбирались под одной блокировкой и удалялись под другой: запись, которую в этот миг добавили в
  избранное, переложили в папку или скопировали снова (воскресили), удалялась, а после `replace`
  удалялись чужие записи восстановленной базы с теми же id. Файлы — после коммита.
- **Удалить недавнее** (`clips_delete_recent`, трей): жёстко, без undo, всё с `copied_at ≥ now − N мин`
  — избранное, папки и уже мягко удалённые тоже (подтверждает UI). Приватное действие: на его время
  `secure_delete = ON`, затем `PRAGMA incremental_vacuum` (до конца — по одной странице за шаг) и
  `PRAGMA wal_checkpoint(TRUNCATE)`; FTS — с `secure-delete`, так что токены пароля не остаются в
  индексе. Одновременные приватные действия считаются (W3): FAST возвращает только последнее. Ещё
  удаляются копии старых данных рядом с базой, сделанные в окне: `backup-before-restore-<ms>\`
  (прежние данные `replace`) и `*.corrupt-<ms>` — в них могло попасть то же, что скопировано за эти
  минуты. Очередь вставки сразу сбрасывает удалённые записи (`paste::stack::prune`) — и после
  команды, и после пункта трея (W4, FIX-13): превью удалённого не остаётся ни в поповере очереди, ни в
  «Очередь: N» трея.
- **Очистить**: `history` — мягко всё, кроме избранного и папок (undo, событие `reset`); `all` — жёстко
  все записи и папки (сниппеты и `apps` остаются), FTS очищается одной командой `'delete-all'` (триггер
  удаления снимается и возвращается в той же транзакции), файлы удаляются, токены отмены сбрасываются,
  приватный режим как у «удалить недавнее»; удаляются **все** `backup-before-restore-*` и
  `*.corrupt-*` (W3: иначе полная прежняя база оставалась на диске); `undoToken: null`.
- **Ретенция** (`lifecycle.rs::run_retention`): только «история» — живые записи с
  `favorite = 0 AND folder_id IS NULL` (избранное, папки, сниппеты не трогаются **и не считаются** в
  лимиты); по возрасту (`copied_at`; месяцы = 30 дней: 1d/1w/1m/3m/6m/1y = 1/7/30/90/180/365 дней) →
  оставить новейшие `maxEntries` → удалять старейшие, пока сумма `size` истории не влезет в
  `maxSizeMb`. Жёстко с файлами, пачками по 500 строк на транзакцию (захват и UI не ждут). Записи, которые
  живой токен отмены ещё может вернуть в папку (вынуты из папки, участники удалённой папки), не удаляются
  и не считаются в лимиты, пока токен жив (W3: иначе Ctrl+Z после «удалить папку, записи оставить» молча
  не возвращал старые записи). `hold_retention`/`release_retention` (W3) приостанавливают ретенцию —
  `replace` держит её, пока не применены настройки копии; проверка и внутри транзакции пачки. Запуск: на
  старте, раз в час, после вставки (не чаще раза в 30 с), сразу при смене `settings.storage`
  (`settings::subscribe` только шлёт сигнал). Событие `clips:changed {origin:'retention',
  change:'deleted', ids}` (ids — если ≤ 1 000).
- **Избранное**: новое — в конец ручного порядка (`max(fav_order) + 1`, в порядке переданных id);
  перенос — дробная середина между соседями, при зазоре < 1e-6 — перенумерация 1, 2, 3…; у каждого
  избранного `fav_order` не NULL (ремонт на открытии). Папки — тот же алгоритм по `position`.
- **Правка текста** (`clip_update_text`): только текстовые виды; новый хэш (`content_hash`), превью,
  метрики, классификация (`capture::classify::classify_text`); **все** богатые форматы (html, rtf и
  PNG-альтернатива с её файлом) сбрасываются — они больше не соответствуют тексту; совпадение с
  другой живой записью → `invalid: duplicate`, с мягко удалённой — та удаляется жёстко.
- **`auto_vacuum = INCREMENTAL`** (решение): включён в v1 (задать можно только до первой таблицы).
  Свободные страницы возвращаются ОС `incremental_vacuum` после приватных действий, после крупной
  ретенции (≥ 100 записей) и раз в час, если свободно ≥ 1 024 страниц (4 МБ). `VACUUM` не делается.
- **Восстановление**: файл, который не открывается как база (`SQLITE_NOTADB`/`SQLITE_CORRUPT`), или
  чужая SQLite-база без нашей версии отодвигаются вместе с `media\`/`icons\` (`*.corrupt-<ms>`) и
  создаются заново; ошибка `SQLITE_CORRUPT` во время работы оставляет маркер `liqlip.db.check` →
  следующий старт делает `quick_check`. База **новее** сборки (`user_version > 2`) не открывается
  (`failed: …`, старт прерывается) — данные не уничтожаются. Liqlip 1.0 базу v2 не откроет
  (сообщит о более новой схеме) — откат версии приложения требует резервной копии.

### 4.2a. Схема v2 (`PRAGMA user_version = 2`, W6-A, 2026-11-27)

Шаг миграции `store/schema.rs::V2`, своя транзакция после шага v1 (у новой базы — оба подряд):

```sql
CREATE TABLE IF NOT EXISTS shot_docs (
  clip_id    INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
  doc        TEXT NOT NULL,     -- AnnotDoc v1 (JSON, §18.6); у снимка без аннотаций — пустой документ
  has_base   INTEGER NOT NULL,  -- 1 = media\base\<id>.png хранит картинку без аннотаций
  saved_path TEXT,              -- последний файл в папке снимков
  updated_at INTEGER NOT NULL   -- последнее изменение картинки / документа (мс) → ?v= ссылок
);
PRAGMA user_version = 2;
```

- `IF NOT EXISTS` — повторный прогон шага безвреден (тест `v1_database_migrates_to_v2`: база v1 с
  записью → v2, запись цела, повторный шаг без ошибки, переоткрытие держит v2).
- `clips.kind` — `TEXT` без `CHECK`: новый вид `screenshot` таблицу не перестраивает. Удаление записи
  удаляет строку каскадом (`foreign_keys = ON`), файл `media\base\<id>.png` — вместе с остальными файлами
  записи (`media::remove_clip_files`), сироты — фоновой уборкой (§4.6).
- `clips.meta.shot` (`ShotMeta {savedPath, edited}`) — зеркало строки для ленты, пишется в той же
  транзакции (`store/shots.rs::sync_meta`); `edited` = в документе есть объекты или обрезка.
- `?v=` у `thumbUrl`/`imageUrl` = `shot_docs.updated_at`, если строка есть, иначе `created_at`
  (`CLIP_COLS` получил подзапрос по первичному ключу `shot_docs`): после правки картинка в шелфе
  обновляется, у записей без строки ссылки прежние.

### 4.3. Порядок ленты и курсор

- По умолчанию: `ORDER BY copied_at DESC, id DESC`.
- `favoritesFirst` (из настроек, читается на каждый `clips_list`): `ORDER BY favorite DESC,
  copied_at DESC, id DESC` — во всех пилюлях, кроме «Избранного».
- Пилюля «Избранное» (`filter.favorites: true`): `ORDER BY fav_order ASC, id ASC`.
- Поиск с текстом: тот же порядок (предсказуемость важнее релевантности).
- Курсор — **непрозрачная строка**: base64url от `r1:<copied_at>:<id>` / `f1:<fav>:<copied_at>:<id>` /
  `m1:<биты f64 fav_order>:<id>`; keyset-условие row-value (`(copied_at, id) < (?, ?)` и т. п.).
  Мусор или курсор **другого** порядка (например, `favoritesFirst` переключили между страницами) →
  `invalid: cursor` (фронт начинает ленту заново). Пустая строка = первая страница.
- `limit`: ≤ 0 → 40, больше 200 → 200; выбирается `limit + 1` строка — `nextCursor` есть, только если
  дальше есть данные.
- Фильтры §9.3 сочетаются через AND; пустой `kinds`/`appIds` = без фильтра; `favorites: false` = без
  фильтра; `since`/`until` — по `copied_at`, включительно.
- `thumbUrl` = `http://liq.localhost/thumb/<id>?v=<created_at>` (только image), `imageUrl` =
  `http://liq.localhost/media/<id>?v=<created_at>`, `iconUrl` = `http://liq.localhost/icon/<id>?v=<8 hex
  от exe>` (только если иконка есть) — `?v=` защищает кэш webview после пересоздания базы.
- `formats`: text-виды — `text` + имеющиеся `html`, `rtf`, `png`; image — `png`; file — `files`, `text`.

### 4.4. Поиск

- Свободный текст режется на термы: пробелы разделяют, `"…"` держит фразу (незакрытая кавычка — до
  конца); управляющие символы — как пробел (W3: NUL обрывал строку FTS5 — `unterminated string`);
  пустые и повторы (без учёта регистра) отбрасываются, не больше 32 термов, каждый ≤ 256 символов
  (W3: вставленный в поиск документ давал фразу из миллионов триграмм и держал блокировку хранилища
  секундами; обрезанный терм находит всё, что находил полный). **Все** термы должны совпасть (AND),
  каждый — как подстрока.
- Терм ≥ 3 символов → `clips_fts MATCH` (title, text, ocr_text — полный текст, а не превью): каждый терм —
  строка FTS5 в кавычках с удвоением `"` (fts5.html §3.1), так что `AND`, `NEAR`, `*`, `:`, `(`, `-`
  пользователя — просто символы. trigram регистронезависим и для кириллицы (зонд W0 + тесты).
- Терм < 3 символов trigram обслужить не может (fts5.html §4.3.5) → `liq_icontains(preview|title, терм)`:
  своя SQL-функция (rusqlite `functions`), регистронезависимая по Unicode (`LIKE`/`lower()` SQLite
  сворачивают только ASCII). Ищет по превью (≤ 512 символов) и заголовку.
- `exclude` — **каждый элемент — один терм** (W3: фронт присылает `-"два слова"` элементом `два слова`;
  раньше он снова резался на слова, и исключалась любая запись с любым из них; мок так и работал):
  кавычки по краям и управляющие символы снимаются; длинные одним `NOT IN (… MATCH "a" OR "b")`,
  короткие — `NOT liq_icontains`. Тест со списком «враждебных» строк (кавычки, `NEAR(`, `%`, `'`, NUL,
  эмодзи, комбинирующие знаки) — ни одна не роняет ни ленту, ни исключения, ни сниппеты.
- Сниппеты (`snippets_list`) — то же по `snippets_fts` (name, body), короткие термы — по name/body.

### 4.5. Медиа и схема `liq`

`store::serve_uri(app, path)` (`media.rs::serve_path`): `/thumb/<id>` — только живая запись `image`
или `screenshot` (W6-A; `/base/<id>` и кадры `/shot/…` отдаёт модуль `screenshot`, §18.3);
нет файла превью → строится из `media\<id>.png` (≤ 360 px по длинной стороне, аспект сохранён,
`image::DynamicImage::thumbnail`; картинка меньше — отдаётся как есть; > 64 МП — отказ) и
записывается; `/media/<id>` — живая запись со строкой `clip_formats` 'png'; `/icon/<appId>` — приложение
с `icon_path`. Иначе `None` (404). MIME `image/png`. Чтение БД — короткая блокировка, файлы — без неё.
`set_app_icon(app_id, png)`: проверка сигнатуры PNG и размера (≤ 1 МБ), `icons\<id>.png` атомарно,
`apps.icon_path`. `InsertOutcome.app_icon_missing` = нет `icon_path` или файла.

### 4.6. Фоновый поток (`maint.rs`, `store::init`)

Поток `liqlip-store`, сигналы по каналу (`Wake::Inserted` — после вставки, `Wake::Settings` — смена
`storage.*`): каждые 30 с — истёкшие токены, чистка мягко удалённых (> 2 мин), маркер повреждения;
ретенция — на старте, раз в час, после вставки (≥ 30 с от прошлой), по смене настроек; раз в час —
`PRAGMA optimize` и `incremental_vacuum` при ≥ 4 МБ свободных страниц; на старте — уборка сирот
(файлы `media`/`thumbs`/`icons` без строки, `media\base\<id>.png` без строки `shot_docs` с `has_base = 1`
(W6-A), `.staging-*` старше часа; каталоги читаются **до** запроса
к БД, поэтому файл свежей вставки не удаляется); затем — разовая дозаливка `meta.dir` у старых файловых
записей (FIX-08, `write.rs::backfill_file_dirs`: живые `kind = 'file'` без `dir`, пачками по 500, только
JSON `meta`, без миграции; при изменениях — `clips:changed {origin:'user', change:'updated'}`). Поток
под супервизором (W3): паника → лог → перезапуск через 5 с (раньше ретенция и чистка молча
прекращались до перезапуска приложения). Ленивое построение превью (`/thumb`) после записи файла
перепроверяет, что картинка жива, — иначе файл удаляется (W3: превью удалённой «удалить недавнее»
картинки не остаётся).

### 4.7. Индексы и планы запросов

Тест `store::feed::plans::hot_statements_use_indexes` проверяет `EXPLAIN QUERY PLAN` (≤ 300 строк, без
стенда) и требует отсутствия голого `SCAN clips`:

| Запрос | План |
|---|---|
| лента (первая страница / с курсором) | `SCAN c USING INDEX idx_clips_feed` / `SEARCH … idx_clips_feed (copied_at<?)` — без сортировки |
| `favoritesFirst` + курсор | `SEARCH … idx_clips_favfirst ((favorite,copied_at)<(?,?))` |
| пилюля «Избранное» | `SEARCH … idx_clips_favfirst (favorite=?)` + сортировка избранных |
| тип (один / несколько) | `SEARCH … idx_clips_kind (kind=? AND copied_at<?)` (несколько — + сортировка) |
| папка / приложения | `SEARCH … idx_clips_folder (folder_id=? AND copied_at<?)` / `idx_clips_app (app_id=?…)` |
| `since`/`until` | `SEARCH … idx_clips_feed (copied_at>? AND copied_at<?)` |
| поиск FTS | `SCAN clips_fts VIRTUAL TABLE` → `SEARCH c USING INTEGER PRIMARY KEY` + сортировка; с типом — `idx_clips_kind` + bloom-фильтр |
| короткий терм / исключения | `SCAN c USING INDEX idx_clips_feed` (по порядку ленты до заполнения страницы) |
| счётчики | `SCAN clips USING COVERING INDEX idx_clips_kind` + `idx_clips_favfirst (favorite=?)`; папки — `idx_clips_folder (folder_id=?)` |
| ретенция (возраст / старейшие / итоги) | `COVERING INDEX idx_clips_history` (`INDEXED BY`) |
| чистка / удалить недавнее | `idx_clips_deleted (deleted_at<?)` / `idx_clips_feed (copied_at>?)` ∪ `idx_clips_deleted` |
| дедупликация | `sqlite_autoindex_clips_1 (hash=?)` |

Известные пределы (по построению, без стенда): поиск с частым термом собирает все совпадения FTS и
сортирует их (≈ число совпадений); короткий терм без совпадений проходит всю ленту по индексу
(превью ≤ 512 символов); после реальных данных `PRAGMA optimize` (раз в час) соберёт статистику для
планировщика.

## 5. Захват (`capture/`)

> Замер: 2026-11-26 (W1-B: реализовано целиком; `cargo test` — 67 тестов модуля зелёные, включая
> стенд 10 копий/с × 10 с: 100/100, задержка копия → запись p50 38 мс, max 47 мс; нативно не
> запускалось — живая проверка по чек-листу в отчёте W1-B). W2-F (FIX-04): тесты модуля не
> зависят от скорости машины (см. «Тесты» ниже); 12 параллельных прогонов `capture::` (и ещё 12 —
> вместе с `paste::`) на 28 потоках — все зелёные, стенд p50 ≈ 38 мс, max < 200 мс.
> W3-A (2026-11-26): `clipboard_text` (FIX-09, `textread.rs`, 8 тестов на фейковом буфере); `cargo test`
> 313/313 дважды, 6 параллельных прогонов `capture::`+`paste::flow`+`tray`+`hotkeys`+`picker::geometry`
> (125 тестов) — зелёные.
> W4-A (2026-11-26): текст из копии с файлами при выключенном типе «Файлы» (FIX-11); тест на фейковом
> буфере; нативно не запускалось.

```
listener-поток `liqlip-clip-listener` — message-only окно (HWND_MESSAGE) + AddClipboardFormatListener
   └─ WM_CLIPBOARDUPDATE → LoopMsg::Notify в канал (оконная процедура больше ничего не делает)
capture-поток `liqlip-clip-capture`
   ├─ дебаунс: 30 мс тишины, но не позже 120 мс от первого события (поток событий не «голодает»)
   ├─ GetClipboardSequenceNumber: уже обработан → дубль; свой (mark_own_write) → выход
   ├─ пауза → выход; «пропустить следующее» → выход + capture:status (номер помечается обработанным)
   ├─ проба владельца: SendMessageTimeoutW(owner, WM_NULL, SMTO_ABORTIFHUNG|SMTO_BLOCK, 250 мс)
   │   не ответил → повтор (без OpenClipboard: зависшего не просим рендерить)
   └─ снимок на отдельном reader-потоке (новый на каждый снимок), ожидание ≤ 1 с на формат, ≤ 3 с всего:
        1. OpenClipboard(NULL): 10 попыток × 25 мс
        2. номер (свой → выход); владелец GetClipboardOwner → GetWindowThreadProcessId →
           QueryFullProcessImageNameW (полный путь, lower); нет владельца → активное окно;
           владелец — окно Liqlip → выход; игнор-лист → выход. Содержимое ещё не читалось
        3. EnumClipboardFormats; маркеры (если privacy.respectMarkers):
           ExcludeClipboardContentFromMonitorProcessing · Clipboard Viewer Ignore — по наличию;
           CanIncludeInClipboardHistory · CanUploadToCloudClipboard — DWORD 0 → выход
           (нечитаемый/короче 4 байт — тоже 0: приватность по умолчанию)
        4. ТОЛЬКО нужные форматы: CF_HDROP (+Preferred DropEffect, ≤ 2000 путей) — и всё;
           иначе CF_UNICODETEXT (≤ 2 МБ + 1 единица UTF-16) → HTML Format (при тексте или картинке)
           → RTF (при тексте); HTML/RTF > 2 МБ отбрасываются целиком → картинка (если тип image
           включён): PNG → CF_DIBV5 → CF_DIB (+ синтез из CF_BITMAP); размер — из заголовка
           ДО копирования, > 64 МП не копируется.
           Тип «Файлы» выключен, а в копии есть CF_HDROP (W4, FIX-11): CF_HDROP не читается;
           есть непустой текст — сохраняется текстовая запись (с её HTML/RTF, без картинки);
           нет текста — выход «type disabled», как раньше
        5. номер ещё раз (рендер delayed-формата во время чтения может его поднять) → CloseClipboard
           (guard: закрывается на любом пути, включая панику)
   └─ занят / владелец не отвечает → повторы через 100, 250, 500, 1000, 2000 мс, затем пропуск с логом
worker-поток `liqlip-clip-worker` (буфер уже закрыт)
   повторная проверка self-copy → основной вид → классификация (classify.rs) → «похоже на секрет»
   (secrets.rs) → тумблеры storage.types → усечение 2 МБ → content::content_hash / image_hash →
   превью, метрики (content.rs) → PNG + миниатюра 360 px (imaging.rs) → Store::insert_clip →
   clips:changed{origin:capture, added|updated} → режим сбора: paste::stack::push(id) →
   app_icon_missing: иконка 64×64 → Store::set_app_icon
```

**Файлы:** `mod.rs` (API §15, потоки + супервизор, `AppSink`), `listener.rs` (окно),
`snapshot.rs` (алгоритм снимка против шва `ClipboardApi`, reader-поток с дедлайнами, фейковый
буфер для тестов), `textread.rs` (текущий текст буфера по запросу — `clipboard_text`, FIX-09),
`win32.rs` (реализация шва, владелец, FileDescription), `pipeline.rs` (capture-цикл,
worker, `build_clip`, стенд), `formats.rs` (разбор CF_UNICODETEXT/CF_HTML/DWORD/заголовков PNG и DIB),
`imaging.rs` (декод PNG/DIB крейтом `image`, нормализация, PNG, миниатюра), `classify.rs`,
`secrets.rs`, `state.rs` (пауза/пропуск/сбор/свои номера), `icons.rs`, `apps.rs`, `commands.rs`.
Вся логика, кроме тонких Win32-обёрток, тестируется на буфере в памяти — системный буфер тесты
не трогают.

**Зависший источник (delayed rendering) — как и почему безопасно.** `GetClipboardData` для
отложенного формата шлёт владельцу `WM_RENDERFORMAT` и ждёт, пока *мы* держим буфер открытым
(learn.microsoft.com «Clipboard Operations → Delayed Rendering»); отменить вызов нельзя. Поэтому:
(1) до `OpenClipboard` владелец проверяется `WM_NULL` с `SMTO_ABORTIFHUNG` — зависшего (или занятого
дольше 250 мс) не просим рендерить, а повторяем позже; (2) сам снимок идёт на отдельном коротком
потоке, capture-цикл ждёт ≤ 1 с на формат (отсчёт от начала каждого вызова) и ≤ 3 с на снимок;
по таймауту формат и все следующие отбрасываются, уже прочитанное используется **только** если
проверки приватности завершились (иначе запись не создаётся); (3) брошенный поток не убивается
(`TerminateThread` портит состояние user32/кучи): он не держит общих блокировок (результат — под
мьютексом, который берётся лишь между вызовами), при возврате вызова видит флаг отмены, пропускает
остальное и закрывает буфер; (4) пока он жив (`reader_busy`), новый снимок не начинается — повтор
по лестнице. Худший случай — источник завис именно во время рендера: буфер занят нашим потоком до
ответа/смерти владельца (как у любого читателя буфера), но Liqlip не висит и следующий снимок
пройдёт сразу после освобождения (тест `hung_delayed_render_does_not_block_the_next_copy`).

**Решения:**
- **Текст + картинка** (Excel/Office): основной — текст, картинка → `NewClip.png` («Вставить как…
  PNG»; только если тип image включён, иначе не читается вовсе). Картинка основная, если текста нет
  (пробельный текст = нет) или HTML-фрагмент — один `<img>` без видимого текста (браузерное
  «Копировать изображение»); нераскодируемая картинка при наличии текста → запись-текст.
- **Self-copy:** вставка (W1-C, `paste::writer`) зовёт `mark_own_write(GetClipboardSequenceNumber())`
  сразу после своего `CloseClipboard`, в том числе после частично неудачной записи (после успешного
  `EmptyClipboard` номер всегда новый); пропускается обновление **ровно с этим номером**, следующее
  (`seq + 1`, даже в том же окне дебаунса) захватывается — тест `own_write_skips_exactly_its_sequence_number`.
  `mark_own_write(seq)` кладёт номер в кольцо из 64 последних; проверка — в
  capture-цикле до чтения, внутри снимка и ещё раз в worker (если `mark_own_write` проиграл гонку
  дебаунсу). Плюс страховка: владелец буфера — окно процесса Liqlip → пропуск. Если владельца нет,
  а впереди окно Liqlip (`echo x | clip` при открытых настройках) — это не «свой», источник неизвестен.
- **Дедупликация событий** — по номеру последовательности (кольцо 32 обработанных, включая номер
  перед закрытием); содержимое на старте приложения не захватывается. Дедупликация записей —
  глобальная, по хэшу, в Store (подъём наверх).
- **Игнор-лист:** запись совпадает с именем файла (`keepass.exe`), с именем без `.exe`
  (`bitwarden`) или с полным путём; без учёта регистра (Unicode), `/` ≡ `\`.
- **Классификация** (SPEC §2.1): цвет — только `#rgb/#rgba/#rrggbb/#rrggbbaa` или `rgb()/rgba()/
  hsl()/hsla()` (запятые или CSS4-пробелы с `/ alpha`, проценты, `deg/turn`; вне диапазона — не
  цвет); `meta.color` — HEX `#RRGGBB(AA)` в верхнем регистре, `rgb(…)`/`rgba(…, a)`, `hsl(…)`/
  `hsla(…, a)` целыми (альфа сохраняется, в моке — отбрасывается). Ссылка — одна строка без пробелов:
  `http/https/ftp/file://` или `www.` (тогда `meta.url = https://…`), `meta.host` как `URL.host`
  (с портом, в нижнем регистре). Контакт — email или телефон целиком (7–15 цифр, `+` или
  разделители; даты, IPv4 и числа с разрядами `1 000 000` — не телефон). Код — ≥ 2 признаков из
  {отступы ≥ 2 строк, токены кода ≥ 2 строк, плотность пунктуации ≥ 5 %} (одна строка — терминатор
  `; { }` + `( = {`). Для rich-копии без ссылки `meta.url/host` берутся из `SourceURL` CF_HTML.
- **«Похоже на секрет»** (только текст целиком после trim, ≤ 16 КБ): карта 13–19 цифр (группы через
  один пробел/дефис) + Луна + первая цифра 2–6 (отсекает IMEI/ID на 0/1/7–9); ключи с префиксами SPEC
  `sk-`, `ghp_`, `gho_`, `github_pat_`, `xoxb-/xoxa-/xoxp-`, `AKIA`+16, `AIza`+35 с алфавитом и длиной
  эмитента и обязательно буквами *и* цифрами (отсекает `sk-learn-…`); PEM-блок `BEGIN … PRIVATE KEY`
  (и `PGP PRIVATE KEY BLOCK`) с совпадающим `END`. `secrets::looks_like_secret` — `pub(crate)`: тем же
  правилом импорт фильтрует тексты из Liquid Tools и `merge` (§17, FIX-01).
- **Лимиты SPEC §2.3:** текст читается до 2 МБ + 1 единица и усекается `content::truncate_text`
  (`meta.truncated`); > 2000 файлов — первые 2000 (лог); картинка > 64 МП — не сохраняется (лог);
  HTML/RTF > 2 МБ — отбрасываются (запись остаётся).
- **Картинки:** PNG и DIB декодируются в RGBA8 (`image` с фичей `bmp`:
  `BmpDecoder::new_without_file_header`, маски V4/V5 по learn.microsoft.com «Bitmap Header Types»;
  32 bpp BI_RGB — непрозрачные); полностью прозрачный результат → непрозрачный (писатели с нулевой
  альфой). Хэш — по пикселям (`image_hash`): одна и та же картинка из PNG и из DIB — одна запись.
  Хранимый PNG перекодируется из RGBA, миниатюра ≤ 360 px (`imageops::thumbnail`; если картинка уже
  меньше — копия PNG).
- **Размер записи** — только байты Liqlip: текст + HTML + RTF + PNG (+ миниатюра); для файлов —
  длина списка путей (размер внешних файлов не читается, к ФС worker не обращается).
- **Пауза** — в памяти (после перезапуска запись идёт): 1…525 600 мин или до возобновления;
  авто-возобновление — поток-таймер, спит шагами ≤ 30 с по настенным часам (после сна ноутбука
  возобновляется вовремя), устаревший таймер (была новая пауза/возобновление) ничего не делает.
  Скопированное во время паузы не захватывается и после неё. «Пропустить следующее» тратится на
  ближайшее внешнее изменение (свои записи и дубли не в счёт); `capture::cancel_ignore_next` (W2-F,
  аддитивно) снимает его без траты — повторный клик по отмеченному пункту трея (§16.3). Каждое
  изменение состояния → `events::capture_status` + `tray::refresh`.
- **Иконки:** `SHDefExtractIconW` 64 px → иначе `SHGetFileInfoW(SHGFI_ICON|SHGFI_LARGEICON)` →
  `GetIconInfo`/`GetDIBits` → RGBA (альфа из AND-маски, если её нет в цвете) → 64×64 PNG; COM
  STA на время вызова; кэш в памяти на exe (PNG + заранее закодированный data-URL; неудача тоже
  запоминается — повторно не извлекается; ≤ 512); в Store — при `InsertOutcome.app_icon_missing`
  (worker, блокирующе, один раз на приложение). **`app_icon_data_url` не блокирует никогда** — его
  зовёт вставка на пути summon шелфа (`paste::focus`, путь exe в нижнем регистре): попадание —
  поиск в карте + копия готовой строки; промах — сразу `None`, извлечение уходит в фоновый поток
  `liqlip-clip-icons` (запросы дедуплицируются маркером «в работе»), следующий summon получает иконку.
  Worker прогревает кэш для каждого приложения, из которого копируют. Блокирующие варианты
  (`icons::png_for`/`data_url`) — только worker и `apps_running`.
- **Имя приложения** — `FileDescription` (`GetFileVersionInfoW`/`VerQueryValueW`, языки из
  `\VarFileInfo\Translation` + запасные 0409/04B0, 0409/04E4), иначе имя файла без `.exe`; кэш.
- **`apps_running`** — `EnumWindows`: видимые, без владельца, не tool-окна (кроме `WS_EX_APPWINDOW`),
  с заголовком, не cloaked (DWM); UWP — процесс дочернего окна `ApplicationFrameHost`; без Liqlip,
  по одному на exe (полный путь, lower), сортировка по имени; на blocking-пуле.
- **Надёжность потоков:** каждый поток в супервизоре (паника/ошибка → лог → перезапуск через 1…30 с,
  каналы сохраняются); снимок и запись обёрнуты `catch_unwind` поштучно; оконная процедура не
  паникует (`try_with`/`try_borrow`). `capture::stop()` (на `RunEvent::Exit`) → `RemoveClipboardFormatListener`
  + `DestroyWindow` + остановка циклов.
- **Логи** — только причины/размеры/виды/время (`skipped: privacy marker`, `image 9000×8000
  exceeds 64 MP`), ни строки содержимого, ни имён файлов, ни заголовков окон.

**API модуля сверх §15 (аддитивно, для координатора):** `capture::stop()` (выход),
`capture::set_collect(app, bool)` (команда `capture_set_collect`), `capture::running_apps()`
(команда `apps_running`), `capture::clipboard_sequence()` (номер для `mark_own_write` без своей
Win32-обвязки), `capture::clipboard_text()` (W3-A, FIX-09 — ниже). Контракт IPC (§9) не менялся.

**Текущий текст буфера — `clipboard_text`** (W3-A, FIX-09; `textread.rs`, команда в `paste/commands.rs`
на blocking-пуле). `{clipboard}` в сниппетах — то, что в буфере **сейчас**, а не свежая запись истории
(та не видит приватных копий KeePass и того, что только что вставил хоткей очереди). Те же правила,
что у снимка, против того же шва `ClipboardApi` (тесты — на фейковом буфере):
- сначала проба владельца `WM_NULL` + `SMTO_ABORTIFHUNG` (250 мс): зависшего не просим рендерить →
  `busy: …not responding`;
- **одно** `OpenClipboard` (10 × 25 мс), читается только `CF_UNICODETEXT` (`IsClipboardFormatAvailable`
  покрывает конвертируемые `CF_TEXT`/`CF_OEMTEXT`), закрытие guard'ом на любом пути;
- чтение — на своём коротком потоке `liqlip-clip-text`, вызывающий ждёт ≤ попытки открытия + 1 с:
  владелец, повисший посреди рендера, стоит одного таймаута (`busy: …in time`), брошенный поток
  закроет буфер, когда владелец ответит (почему не убиваем — см. «Зависший источник» выше);
- текст до первого NUL, ≤ 2 МБ (`content::truncate_text`, по границе символа); нет текстового
  формата (картинка, файлы) → `null`; занятый буфер → `busy: …held`;
- **маркеры приватности не применяются**: это явное действие пользователя (вставка сниппета), текст
  не сохраняется и не логируется.
Фронт (W3-C) при `null` или ошибке берёт прежний запасной путь — свежую запись истории.

**Тесты** (`cargo test capture::`, 67): классификация (цвета/ссылки/контакты/код/края), секреты
(карты/ключи/PEM/ложные), CF_HTML и заголовки, состояние (пауза, таймер, пропуск, свои номера),
снимок на фейковом буфере (один OpenClipboard, только нужные форматы, приоритеты, маркеры — 7 случаев
вкл. Bitwarden и битый DWORD, игнор-лист, свой процесс, занятый буфер, лимиты, зависший формат и
зависший маркер), worker (`build_clip`: поля, секреты, тумблеры, усечение, Excel текст+PNG, «Копировать
изображение», дедуп PNG≡DIB, файлы), конвейер (старт, дубли событий, self-copy, пауза, пропуск,
режим сбора, повторы при занятом буфере и неотвечающем владельце, игнор/маркеры, зависший рендер,
поток событий, self-copy ровно по номеру), Win32-обёртки (зарегистрированные форматы, свой exe,
FileDescription, иконка explorer.exe 64×64, неблокирующий `app_icon_data_url` (промах → сразу `None`)
и фоновое извлечение, listener — `WM_CLIPBOARDUPDATE`, отправленное самому себе, и остановка,
`apps_running`), **стенд** `bench_10_copies_per_second_for_10_seconds`: 100 записей за 10 с из
другого потока → 100/100 по порядку, все хэши разные, задержка p50 38 мс / max 47 мс (норма SPEC §7 —
≤ 300 мс; тест держит в норме медиану, максимум — < 2 с).

**Тесты не зависят от скорости машины** (FIX-04, W2-F):
- тесты содержимого снимка идут с запасом 10 с на формат / 20 с на снимок (`generous()`), тесты
  дедлайнов — с боевыми 1 с / 3 с;
- «зависший владелец» — затвор фейкового буфера (`FakeClipboard::hang`/`release`), а не задержка:
  проверяется, что снимок вернулся, пока владелец ещё висит, без замера времени;
- конвейер ждёт условий, а не спит: старт цикла (его первое чтение номера, раньше — `sleep(50)`,
  из-за которого поздно стартовавший цикл принимал первую копию теста за «содержимое при старте»),
  `Harness::settle` («цикл дочитал изменение»: повторное событие и ещё одно чтение номера),
  `wait_until`; все ожидания — до 10 с;
- фейковый буфер пишет «как настоящее приложение» (`set_text_when_closed`): только пока снимок не
  держит буфер открытым. Прежняя запись посреди снимка поднимала номер, и читатель принимал его за
  подъём от отложенного рендера своего же чтения — копия помечалась обработанной, не будучи
  прочитанной (в Windows так не бывает: `OpenClipboard` приложения не пройдёт, пока буфер открыт);
- стенд: следующая запись ждёт, пока прочитан текст предыдущей (≤ 2 с), и своего 100-мс такта — на
  машине, которая успевает, это ровно 10 копий/с, а на перегруженной копия не затирается до того, как
  её вообще можно было прочитать (буфер хранит только последнее);
- иконки проверяются по состоянию кэша, а не секундомером.

Замер: 12 параллельных прогонов `capture::` (+ 3 прогона `store/import/settings`) на 28 потоках —
зелёные, дважды; до правки в таком прогоне падали стенд («потерянная копия #2» — запись посреди
снимка) и `own_write_…` (поздний старт цикла).

**Не проверено нативно (риски):** реальные `WM_RENDERFORMAT`-задержки Office и поведение
`SMTO_ABORTIFHUNG` с занятыми, но живыми источниками; точное поведение номера последовательности
при рендере (документировано, что он растёт); маски CF_DIBV5 от разных писателей.

## 5a. Распознавание текста на картинках (`ocr/`)

> Замер: 2026-11-26 (W2-A: реализовано целиком — движок, фоновая очередь, команды, мок;
> `cargo test` — 266 в крейте, из них 27 OCR, включая настоящий Windows OCR на картинке,
> нарисованной тестом в памяти; нативное приложение не запускалось — сценарии живой проверки в
> отчёте W2-A). W3-B (2026-11-26): результат задания, взятого до `replace`, отбрасывается
> (поколение базы, `Store::generation`); тест `a_result_for_a_replaced_database_is_dropped`.

Файлы: `ocr/engine.rs` (WinRT-распознаватель и чистые помощники), `ocr/worker.rs` (цикл очереди и
блок управления), `ocr/mod.rs` (запуск, подписка на настройки, логика команд, прямое
распознавание), `ocr/commands.rs` (§9.6), `store/ocr.rs` (SQL очереди внутри хранилища).

### 5a.1. Движок (`engine.rs`)

```
PNG (media\<id>.png) → image: декод → наложение на белый (прозрачность) → 8-битный серый (Rec.601)
  → план размера (plan) → на каждую плитку: DataWriter.WriteBytes → DetachBuffer (IBuffer)
  → SoftwareBitmap.CreateCopyFromBuffer(Gray8, Ignore) → OcrEngine.RecognizeAsync().join()
  → OcrResult.Lines → OcrLine.Text → нормализация → строки через \n
```

- **Перенос** цепочки Liquid Tools (`git show 0295998:src-tauri/src/ocr.rs`) без WinRT-декодера:
  декодирует крейт `image` — один путь для любого размера, масштаб до того, как пиксели попадут в
  WinRT.
- **Серый, а не BGRA.** Документация `RecognizeAsync` не перечисляет форматы; Gray8 проверен
  тестом с настоящим движком (Windows 11) и в 4 раза меньше BGRA8: картинка 64 МП (предел захвата)
  — 64 МБ на копию вместо 256. Если движок не примет Gray8, битмап один раз конвертируется в BGRA8
  (`SoftwareBitmap.Convert`) и распознаётся снова.
- **Язык** (`settings.ocr.language`): `auto` → `TryCreateFromUserProfileLanguages`; если ни один
  язык профиля не разрешается в установленный OCR-язык (null), берётся первый из
  `AvailableRecognizerLanguages` — распознавание всё равно работает. Конкретный тег — только из
  `AvailableRecognizerLanguages` (без учёта регистра) → `TryCreateFromLanguage(new Language(tag))`;
  тег, чей пакет удалён, откатывается на `auto` (в логе — «ru unavailable → en-US»). windows-rs
  отдаёт null-класс как `Err` с пустой ошибкой (`windows-core Type::from_abi`), поэтому `Err` от
  `TryCreate*` = null. Языков нет вовсе → `Unavailable::NoLanguages` → `available: false`.
- **Размер.** `OcrEngine.MaxImageDimension` читается при каждом создании движка (на Windows 11 —
  10 000 px). Короткая сторона больше предела → картинка уменьшается с сохранением пропорций
  (`imageops::resize`, Triangle). Картинка, длинная только по одной оси (страница целиком
  1 200 × 30 000, три монитора 11 520 × 2 160), **не ужимается до нечитаемого**: режется на плитки
  ≤ предела по длинной оси с перекрытием 256 px (`plan`); каждое слово оставляет ровно одна плитка
  — та, которой принадлежит центр слова (`OcrWord.BoundingRect`, `owns`), поэтому на разрезе
  ничего не теряется, а в перекрытии не дублируется (тест `every_word_belongs_to_exactly_one_tile`
  + проверка на настоящем движке). Строка, целиком принадлежащая плитке, берётся как
  `OcrLine.Text`; разрезанная границей владения — своими словами через пробел. Не больше 12 плиток
  (≈ 120 000 px при пределе 10 000); длиннее — масштаб, чтобы влезло.
- **Текст:** пробельные серии внутри строки → один пробел, строки обрезаются, пустые
  выбрасываются, склейка через `\n`; слова строки — как их отдаёт `OcrLine.Text` (для языков без
  пробелов расстановку решает Windows). Не больше 256 КБ (по границе символа). Картинка без текста
  → `done` с `""` — это не ошибка.
- **Апартамент.** Каждый поток, который трогает WinRT, сначала входит в MTA (`Apartment` =
  `RoInitialize(RO_INIT_MULTITHREADED)`; успешный вызов, включая `S_FALSE`, уравновешивается
  `RoUninitialize`; `RPC_E_CHANGED_MODE` не ошибка — объекты агильные). `OcrEngine` —
  `MarshalingType.Agile`, `ThreadingModel.Both`.
- **Приоритет.** Декод, масштаб и распознавание идут под `THREAD_PRIORITY_LOWEST`
  (`low_priority`), после чего приоритет потока возвращается: обращения к базе (под мьютексом
  хранилища) — с обычным приоритетом, захват никогда не ждёт «голодающий» OCR-поток. Сам
  `RecognizeAsync` может выполняться на потоках пула WinRT — их приоритет Liqlip не задаёт.

### 5a.2. Состояния записи и очередь в базе (`store/ocr.rs`)

Очередь — **это база**: картинка с `ocr_state = pending` ждёт распознавания, поэтому очередь
переживает перезапуск. Колонки — из схемы v1 (§4.2), схема не менялась.

| `ocr_state` | `Clip.ocr` | Смысл |
|---|---|---|
| 0 | `none` | не в очереди: любая не-картинка; картинка, снятая при выключенном OCR, или импортированная |
| 1 | `pending` | ждёт; `ocr_text` может хранить прежний текст (переиндексация) — поиск по нему работает |
| 2 | `done` | `ocr_text` записан (`""` — текста нет) |
| 3 | `error` | распознавание не удалось; прежний `ocr_text` сохраняется; повтор — только `ocr_reindex` / `clip_ocr` |

- **Вставка из захвата** (`write.rs`, две строки W2-A): новая картинка — или повторно скопированная,
  которая ни разу не стояла в очереди (`none`), — получает `pending` **в той же транзакции**, если
  OCR включён (`OcrHook.queue_new`); `done`/`error` при повторном копировании не трогаются. После
  коммита вставки картинки — `ocr_wake()` (будит очередь). Флаг и будильник хранятся в поле
  `Store.ocr` (`OcrHook`), их ставит `ocr::start` и подписка на настройки.
- **Запись результата** (`ocr_finish`) — один `UPDATE`: `Ok(text)` → `ocr_text = text, ocr_state = 2`;
  ошибка → `ocr_state = 3`. Условие — живая картинка (`kind = 'image' AND deleted_at IS NULL`); для
  очереди ещё и `ocr_state = 1` (результат `clip_ocr`, успевший раньше, побеждает). Ноль строк =
  запись удалена (мягко или жёстко) во время распознавания → результат **молча выбрасывается**.
  **Поколение базы** (W3): `OcrJob.generation` = `Store::generation()` в момент взятия задания
  (`ocr_next`/`ocr_job`); `ocr_finish_job` под блокировкой соединения сравнивает его с текущим и при
  несовпадении ничего не пишет. Успешный `replace` (§17.4) увеличивает поколение — иначе текст картинки
  из прежней базы записался бы в запись восстановленной базы с тем же id (очередь и `clip_ocr`).
  `clips_fts` переиндексирует `ocr_text` триггером `clips_fts_au` — текст сразу ищется (§4.4).
- **Запросы очереди** (`ocr_next` — новейшая ожидающая, `ocr_pending` — счётчик) идут по
  `idx_clips_kind (kind = 'image')` — посещаются только картинки, по порядку ленты, без сортировки
  (тест `queue_statements_use_the_kind_index`). Отдельный индекс по `ocr_state` не нужен.
- `ocr_requeue_all` (переиндексация): все живые картинки → `pending`, тексты остаются до замены.
  `ocr_enqueue_unprocessed` (догон): живые `none`-картинки → `pending`.

### 5a.3. Фоновая очередь (`worker.rs`, поток `liqlip-ocr`)

Один поток (MTA, под супервизором: паника → лог → перезапуск через 1…30 с), одна картинка за раз,
новейшие первыми. Цикл: OCR выключен → движок освобождается, сон до пробуждения → догон
(если взведён) → `ocr_next` → нет работы: движок освобождается (его языковые данные не держатся в
простое), завершается идущая переиндексация, сон → движок для текущего `ocr.language` (создаётся
лениво на пачку работы; нет языков → запись остаётся `pending`, повторная проверка раз в 60 с —
языковой пакет, установленный при запущенном Liqlip, подхватывается без перезапуска) → чтение PNG
→ распознавание → проверка эпохи → `ocr_finish` → событие.

- **Пробуждение:** вставка картинки (хук хранилища); смена `ocr.enabled`/`ocr.language`
  (`settings::subscribe` — только флаги и сигнал, подписчик работает под замком настроек);
  `ocr_reindex`; старт; `ocr::wake(app)` (для импорта W2-B: будит и взводит догон); плюс сон не
  дольше 5 мин в простое.
- **Догон** (`none` → `pending`): на старте при включённом OCR, при включении OCR, по
  `ocr::wake`. Поэтому картинки, снятые при выключенном OCR или импортированные, распознаются,
  как только OCR включён.
- **Эпоха.** Растёт при выключении OCR, смене языка и старте переиндексации. Результат задания,
  начатого в старой эпохе, выбрасывается, запись остаётся `pending` и переделывается в новых
  условиях (новым языком; после включения). Смена языка ранее распознанное **не трогает** — новый
  язык применяется к новым картинкам и при «Распознать заново».
- **Ошибки:** нет PNG / битый PNG → `error`; сбой движка → `error`, три сбоя подряд → движок
  пересоздаётся; ошибка базы → запись остаётся `pending`, повтор через 5 с.
- **События:** `clips:changed { origin: 'user', change: 'updated', ids }` — пачкой, не чаще раза в
  400 мс (шелф обновляет карточку и поиск); `ocr:progress` — во время переиндексации, не чаще раза
  в 150 мс, последнее событие всегда `done == total`.
- **Логи:** id, длительность, код ошибки, язык движка, итоги переиндексации — никакого
  распознанного текста. Успешные распознавания пишутся, только если дольше 5 с.

### 5a.4. Команды (`commands.rs` → `mod.rs`, всё на blocking-пуле)

| Команда | Поведение |
|---|---|
| `ocr_status` | `{ available, languages, pending }`: языки — теги BCP-47 `AvailableRecognizerLanguages` (читаются заново при каждом вызове), `available = languages ≠ []`, `pending` — живые картинки в очереди (в том числе при выключенном OCR). Сбой WinRT → `failed:` |
| `ocr_reindex` | OCR выключен → `invalid:`; переиндексация уже идёт → не перезапускается, текущий прогресс отправляется ещё раз, `Ok`; нет языков → `failed:`; иначе все картинки → `pending`, сразу `ocr:progress {0, total}` (`{0, 0}`, если картинок нет — больше ничего не идёт), дальше прогресс от очереди до `done == total`. Если за время переиндексации пришли новые картинки, `total` растёт. Проверка → requeue → старт сериализованы замком (двойной клик не удваивает) |
| `clip_ocr(id)` | нет записи / удалена → `not_found:`; не картинка → `invalid:`; `done` → текст из базы сразу (и при выключенном OCR — распознанное остаётся); иначе OCR выключен → `invalid:`; иначе распознать сейчас, вне очереди, на своём потоке `liqlip-ocr-now` с ожиданием ≤ 10 с: результат пишется как результат очереди (`done`/`error` + `clips:changed`), удалённая за это время запись → `not_found:`; нет языков → `failed:` (запись не трогается); таймаут → `failed: … timed out`, а работа доводится и результат сохраняется. Одновременные вызовы для одного id ждут одно задание; больше 4 разных картинок сразу → `busy:` |

### 5a.5. Мок (`src/lib/ipc/mock/system.ts`)

Повторяет поведение: `ocr_status` — `en-US`, `ru-RU`, `pending` — по сиду; очередь мока
(шаг 160 мс, 1 мс в node-тестах) переводит `pending` → `done` с осмысленным текстом (EN/RU, часть
картинок — без текста) и шлёт `clips:changed`; `ocr_reindex` — прогресс `{0,total}` → `done == total`,
без удвоения, OCR выключен → `invalid`; `clip_ocr` — текст для любой картинки сида (сохраняется,
событие), ошибки по кодам; включение OCR догоняет `none`-картинки. Флаг `?mock-ocr=nolang` —
компьютер без языковых пакетов (`available: false`, переиндексация и распознавание → `failed`).
`?mock-unimpl=ocr` по-прежнему делает команды `unimplemented`. Тест — `tests/ocrMock.test.mjs`.

### 5a.6. Ограничения

- Языки — только установленные языковые пакеты Windows с компонентом OCR (Параметры → Время и
  язык → Язык и регион). Движок одного языка: кириллический движок читает латиницу с ошибками
  (`l`/`I`) и наоборот — для этого в настройках есть выбор языка.
- Gray8 подтверждён на Windows 11 (запасной путь BGRA8 — на случай других сборок).
- Крупная картинка (до 64 МП) кратковременно занимает память: декод RGBA + серый (≈ 1,25 × RGBA).
- Нативно (в приложении) не проверялось: прогресс в окне настроек, обновление карточек,
  поведение при установке/удалении языкового пакета на лету.

## 6. Вставка (`paste/`)

> Замер: 2026-11-26 (W1-C, ветка `w1/paste`): модуль реализован целиком; `cargo test` — 49 тестов
> чистой логики модуля (73 в крейте); нативно не запускалось (PQ-02) — фокус и ввод проверяются по
> чек-листу из отчёта W1-C на INT-1. W2-F: HUD называет сочетание вставки цели (FIX-07, §6.6);
> тесты модуля по-прежнему без привязки к таймингам (проверено параллельными прогонами).
> W3-A: главная клавиша хоткея очереди — разбором движка хоткеев (§6.5); копия без цели закрывает шелф с
> возвратом фокуса; зажатый > 1 с триггер не отдаёт фокус и окну администратора; флаг передачи
> keepOpen снимается guard'ом; «Открыть с помощью…» прячет шелф (§6.8); команда `clipboard_text` (§5).
> W4-A (2026-11-26): очередь чистится сразу после «Удалить недавнее» из трея (FIX-13, §4); тест
> `prune_from` на временном хранилище.

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

**Главная клавиша хоткея очереди** (её ждёт `WaitKeys`) — `hotkeys::main_vk`, тем же разбором, что
регистрирует сочетание (W3-A). Прежний разбор в `input.rs` знал только буквы, цифры, F1–F24 и пару
имён: при `Ctrl + Num5`, стрелках, `-` и т. п. ожидания не было, и автоповтор клавиши после снятия
модификаторов печатал её в цель. `paste::mask_modifier_release()` (W3-A) — одно нажатие маски `0xE8`
для движка хоткеев (§8).

### 6.6. Поток (`flow.rs`)

Чистый планировщик `plan(origin, route, closeOnCopy) → [Step]` (порядок закреплён тестами) +
исполнитель; вставки сериализованы (вторая ждёт первую).

| Откуда / цель | Шаги |
|---|---|
| шелф, цель вставляемая | `Write → WaitKeys → FocusTarget → HideShelf → Settle(30 мс) → Inject → After → HudIfNotPasted` |
| шелф + `keepOpen` | `Write → WaitKeys → BeginHandoff → FocusTarget → Settle → Inject → After → ReturnToShelf → EndHandoff` |
| шелф, цель elevated | `Write → WaitKeys → FocusTarget → HideShelf → Hud(elevated)` → `copied/elevated` (фокус возвращён окну админа — Ctrl+V одним нажатием; `SetForegroundWindow` UIPI не блокирует, ввод — блокирует) |
| шелф, цели нет | `closeOnCopy` → `Write → CloseShelf → Hud(copied)` (закрытие как по Esc, с возвратом фокуса — W3-A, SPEC §3.3; так же, как `Ctrl+Enter`); иначе `Write` → `copied/no_target` |
| `keepOpen` + elevated / нет цели | `Write` (шелф виден — тост показывает фронт) |
| без шелфа (хоткей очереди), цель = текущее активное окно | `Write → WaitKeys → FocusTarget (сразу ок) → Settle → Inject → After → HudIfNotPasted`; elevated → `Write → Hud(elevated)`; нет цели → `Write → Hud(copied)` |

Почему так (отклонения от первоначального эскиза «hide → focus → запись»):
- **Write первым**, пока шелф виден: занятый буфер (`failed/clipboard_busy`) обнаруживается до
  того, как что-то сдвинулось, и шелф может показать ошибку. От записи до ввода всё равно ≥ 30 мс.
- **WaitKeys**: ждём (≤ 1 с), пока отпущены Enter, 1–9 (и NumPad 1–9), главная клавиша хоткея
  очереди — иначе автоповтор после смены фокуса попал бы в цель («двойной Enter», «вставилась v»).
  Не отпущены за 1 с → фокус не переводим и ввод не посылаем (`copied/focus_lost`; у окна
  администратора — `copied/elevated`): W3-A распространил это на elevated-маршрут — раньше он отдавал
  фокус окну администратора и с зажатым Enter, и автоповтор доходил, например, до админ-консоли.
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
  `SetForegroundWindow(шелф)`. Флаг снимается на любом выходе из `run`, в т. ч. при панике шага
  (guard, W3-A): иначе шелф навсегда перестал бы закрываться по клику мимо. Если Windows не вернёт активацию (наш процесс уже не foreground),
  шелф остаётся видимым, но пассивным — клик в него активирует.
- Исход: `pasted`; `copied` + `no_target | elevated | focus_lost`; `failed` + `clipboard_busy |
  not_found | stack_empty`; `partial` (аддитивно в контракт, W1-C) — к `pasted/copied`, если часть
  записей пропущена.
- **HUD называет сочетание цели** (FIX-07, W2-F): тосты `copied` и `elevated` при известной цели
  получают в `HudShow.text` сочетание, которое вставляет в этом приложении (`input::resolve_chord` →
  `input::chord_label`: «Ctrl+V» / «Shift+Insert» / «Ctrl+Shift+V»), — то же, что ушло бы в
  `SendInput`; без цели `text` нет, и HUD пишет «Ctrl+V».

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
  отмена диалога — не ошибка. **Перед диалогом шелф прячется** (`picker::hide_now`, W3-A): диалог —
  окно нашего процесса, выход шелфа по потере фокуса его пропускает, а always-on-top шелф закрывал
  центрированный диалог целиком — пользователь не видел ничего.
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
  буфере, HUD подсказывает сочетание вставки этого приложения.
- `keepOpen`: возврат активации шелфу зависит от правил foreground Windows (см. 6.6).
- Внутри окна-цели фокус возвращается в тот элемент, который приложение само восстанавливает при
  активации (как при Alt+Tab).

## 7. Окно шелфа (`picker/`)

> Замер: 2026-11-26 (W1-D: реализовано целиком; `cargo test` — 47 тестов чистой логики модулей
> `picker`/`hotkeys`; нативно не запускалось — чек-лист живой проверки в отчёте W1-D). W3-A: `picker::close`
> (копия без цели), HUD не паникует на пустой рабочей области; нативно не запускалось. W4-A (2026-11-26):
> повтор хоткея активирует пассивный шелф (FIX-10, 3 теста машины); нативно не запускалось.

Перенос логики Liquid Tools `src-tauri/src/screenshot/shotlib.rs` (докинг по физическому `rcWork`,
строгий hover-триггер, `BLEED = 72`, отложенный `hide` 260 мс, двухскоростной опрос) с новой
моделью состояния. Файлы: `machine.rs` (стейт-машина, чистая), `geometry.rs` (докинг/hover/HUD,
чистая), `mod.rs` (диспетчер + исполнитель эффектов, API §15), `poll.rs` (фоновый опрос),
`win.rs` (тонкие Win32-обёртки), `hud.rs`, `commands.rs`.

### 7.1. Стейт-машина

Одно состояние `Machine` в одном `Mutex` (+ `Condvar` для опроса); события → эффекты; исполнитель
выполняет эффекты **на главном потоке** (`run_on_main_thread`; там tao применяет флаги окна
синхронно, поэтому эффекты идут строго по порядку, а `WS_EX_TOOLWINDOW` возвращается сразу после
каждого изменения флагов). Замок не держится во время эффекта (вызов окна может синхронно породить
событие). Результат активации приходит обратно событием `Activated(ok)`.

```
            Config(hover вкл, край)                     HoverTrigger (курсор упёрся в край ≥200 мс)
  Hidden ─────────────────────────────▶ Armed ──────────────────────────────▶ Open{Hover, passive}
    │  ◀── Config(hover выкл) ───────────┘ │                                     │ MakeInteractive /
    │ Summon                        Summon │                                     │ Summon (апгрейд)
    ▼                                      ▼                                     ▼
  Open{Summon, interactive = SetForegroundWindow удался}  ◀─────────  Open{_, interactive}
    │ Hide(Esc) · Summon(повтор, interactive) · ClickOutside · Blur(interactive)
    │ · CursorLeft(passive hover) · CloseRequested(Alt+F4) · ShowSettings
    │                                                  HideNow (вставка: нативный hide сразу)
    ▼                                                                    │
  Closing ── 260 мс (HideTimer того же поколения) ──▶ Armed, если hover вкл., иначе Hidden
    └── Summon в эти 260 мс отменяет скрытие (поколение +1) и открывает снова
  Open{_, interactive} ── Deactivated (фокус ушёл при keepOpen-вставке / drag-out) ──▶ Open{_, passive}
  Open{Summon, passive} ── Summon(повтор) ──▶ активация: удалась → interactive, отказ → Closing (FIX-10)
```

- **Armed** — окно показано, click-through, не фокусируемое (`WS_EX_NOACTIVATE`), фронт в закрытом
  состоянии (панель за краем); опрос следит за краем. Только для краёв и при `hoverOpen`.
- **Хоткей при открытом hover-шелфе** не закрывает, а апгрейдит до `Summon` с активацией
  (пользователь хочет клавиатуру) и шлёт второй `picker:open {mode:'summon'}`.
- **Повтор хоткея (клик по трею, второй экземпляр) при пассивном summon-шелфе** (W4, FIX-10; Windows
  отказал в активации или фокус ушёл при keepOpen-вставке / drag-out) тоже не закрывает: тот же путь,
  что апгрейд hover — `CaptureTarget` → `ShowActive` (после `WM_HOTKEY` / клика по трею процесс получил
  последний ввод и вправе взять foreground) → второй `picker:open {mode:'summon', interactive:true}`;
  фронт обновляет `interactive` и фокусирует поиск, поиск и выбор не сбрасываются. Если Windows снова
  отказал — повтор закрывает шелф (`Repeat`), иначе хоткей стал бы пустым нажатием. Закрывает повтор
  только активный шелф. `open_seq` растёт — ожидающий «клик вне» от клика по трею сбрасывается.
- **Alt+F4**: `lib.rs` прячет окно (`CloseRequested` → `hide`), второй слушатель модуля переводит
  машину в `Closing`; страховка — `Summon { visible }`: если машина «открыта», а окно невидимо,
  summon открывает, а не закрывает.

### 7.2. Summon и фокус

Порядок: `CaptureTarget` (до показа: `GetForegroundWindow` — куда вернуть фокус, кроме самого
шелфа/HUD/панели задач; `paste::focus::capture_target()` — цель вставки, и при summon, и при hover)
→ `Dock` (+ `picker:layout`) → `ShowActive`: `set_ignore_cursor_events(false)`, `set_focusable(true)`,
`show()`, затем [`SetForegroundWindow`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
с проверкой `GetForegroundWindow` → `picker:open { mode, target, interactive }`.

- Право на foreground: «The calling process received the last input event» — после `WM_HOTKEY`,
  клика по иконке трея, выбора пункта меню. **Ничего не форсируем**: без `AttachThreadInput`, без
  `SPI_SETFOREGROUNDLOCKTIMEOUT`, без инъекции Alt (tao `set_focus` делает Alt-трюк — шелф его не
  вызывает). Отказ Windows (например, второй экземпляр из ярлыка — права у него, а не у нас) → шелф
  открывается пассивным (`interactive:false`); клик по нему активирует окно (`Focused(true)` →
  interactive), клик в поиск → `picker_make_interactive`.
- **Возврат фокуса** (`RestoreFocus`): `SetForegroundWindow(запомненное окно)`, только если шелф ещё
  держит фокус (или foreground пуст) и окно живо (`IsWindow`) — клик в другое окно уже отдал фокус
  туда, куда хотел пользователь. Окно настроек — допустимая точка возврата.
- `hide_now()` (вставка, W1-C §6.6: вызывается после `SetForegroundWindow(цель)`) — `HideNow`:
  нативный `hide` сразу, без анимации и без возврата фокуса; идемпотентен (в `Hidden`/`Armed` —
  ничего, в `Closing` после выхода по потере фокуса — тоже прячет сразу); блокирует вызывающего
  ≤ 1 с, работает и с главного потока (`run_on_main_thread` там выполняется сразу). Если hover
  включён, через 260 мс окно снова взводится.
- **keepOpen-вставка / drag-out** (W1-C): пока `paste::focus::handoff_active()` или
  `paste::dragout::active()`, выход по потере фокуса не срабатывает и фокус не возвращается; если
  после них фокус к шелфу не вернулся — `Deactivated`: шелф остаётся открытым и пассивным (клик
  активирует). Модальный цикл drag-out идёт на главном потоке, и `Focused(false)` может прийти уже
  после него — опрос запоминает, когда последний раз видел активную передачу/перетаскивание, и
  сторож считает потерю фокуса в течение 0.8 с после этого той же «передачей».

### 7.3. Выходы (одна воронка `close(reason, restore)`)

| Выход | Как ловится | Возврат фокуса |
|---|---|---|
| Esc | фронт → `picker_hide(true)` | да |
| Повтор хоткея / клик по трею | `Summon` при `Open{Summon, interactive}`; пассивный — сначала активация, закрытие только при повторном отказе Windows (FIX-10) | да |
| Клик вне панели | опрос кнопок мыши (30 мс): нажатие вне физического rect панели (в т. ч. по прозрачному bleed); закрытие через 100 мс после отпускания (кликнутое окно успевает активироваться, клик по трею первым отрабатывает свой toggle) или через 400 мс удержания; клик по диалогу своего процесса (не Tauri-окно) игнорируется; во время drag-out/передачи фокуса не отслеживается | да, если фокус ещё у шелфа |
| Потеря фокуса | `WindowEvent::Focused(false)` для interactive → поток-сторож: ждёт конца передачи фокуса / drag-out / удержания кнопки (≤ 15 с) + 100 мс; foreground снова шелф или окно своего процесса (диалог, настройки) — не закрывает; после передачи/drag-out — `Deactivated` вместо закрытия | нет |
| Вставка | `hide_now()` | нет (возвращает W1-C) |
| Копирование без цели (Enter/клик, `closeOnCopy`) | `picker::close(app, true)` из потока вставки (W3-A) | да, как Esc |
| Уход курсора | только `Open{Hover, passive}`: курсор вне окна, продлённого до истинного края экрана | нет |
| Alt+F4 | `CloseRequested` | да, если foreground пуст/шелф |
| Открытие настроек | `show_settings` закрывает шелф до показа окна настроек | нет |

Закрытие: `picker:close` → окно click-through → через 260 мс нативный `hide` (или повторное
взведение). Почему опрос, а не хук мыши: пассивный шелф не получает `Focused(false)`, а клики по
прозрачному bleed приходят в webview; LL-хуки зарезервированы за движком хоткеев (§8).

### 7.4. Докинг

`geometry::dock` (физические px; процесс Per-Monitor V2 — tao `become_dpi_aware` →
`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`, проверено по исходнику tao 0.35.3): панель по
расположению (center 760×640, `centerRow` 760×360, left/right 400×826, top/bottom 800×360 —
логические × масштаб монитора), прижата к краю `rcWork`, центрирована по другой оси, ужата до рабочей
области; окно = панель + `BLEED` (72 × масштаб) на недокованных сторонах (центр — со всех). Монитор
(`pick_monitor`): `cursor` — под курсором (в щели между мониторами — ближайший), `primary`, имя
устройства GDI (`\\.\DISPLAY2`; отключён → основной). Мониторы — `available_monitors()` Tauri
(`rcMonitor`; `work_area` = `GetMonitorInfoW::rcWork`; `scale_factor`). Применение: позиция → размер
→ позиция (на мониторе с другим масштабом tao обрабатывает `WM_DPICHANGED` на первом перемещении и
может сдвинуть окно в предложенный Windows rect; размер и второе перемещение ставят точную
геометрию), после показа геометрия перепроверяется. `picker:layout` (логические ширина/высота панели
и bleed) — после каждого докинга, до показа.

Без перезапуска: опрос сравнивает сигнатуру дисплеев (`SM_CMONITORS`, виртуальный экран,
`rcMonitor`/`rcWork`/эффективный DPI докованного монитора) → `Redock`; во взведённом состоянии с
`monitor: cursor` край следует за монитором курсора. `settings.picker.{position, centerRow, monitor,
hoverOpen}` применяются на лету (`settings::subscribe` → `Config`).

### 7.5. Hover

Строгий триггер (`HoverTracker`): курсор на крайнем физическом пикселе истинного края монитора
(`rcMonitor`, не `rcWork` — ниже панели задач), в пролёте панели (углы свободны), без зажатой кнопки,
≥ 200 мс; дошёл до края с зажатой кнопкой (перетаскивание окна) или шелф только что закрылся —
триггер разряжен, пока курсор не уйдёт с края. **Стык с соседним монитором** (`edge_is_seam`: другой
монитор продолжается за этим краем в пролёте панели) — триггера нет. Опрос: 60 мс в полосе 240 px у
края, 250 мс иначе; 30 мс, пока шелф открыт; спит (condvar), пока скрыт.

### 7.6. Alt+Tab и панель задач

`WS_EX_TOOLWINDOW` включается, `WS_EX_APPWINDOW` снимается (tao ставит его всем окнам без владельца —
иначе окно вернулось бы в Alt+Tab) **до и после** каждого вызова tao, меняющего флаги
(`show`/`hide`/`set_focusable`/`set_ignore_cursor_events` переписывают `GWL_EXSTYLE` в `apply_diff`),
затем `SetWindowPos(SWP_FRAMECHANGED)` («changes … will not take effect until you call
SetWindowPos», [SetWindowLongPtrW](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowlongptrw)).
До — чтобы оболочка увидела tool window в момент показа, после — потому что tao только что
переписал стиль.

### 7.7. HUD (`hud.rs`)

Окно `hud` показывается **только** через Win32 `SetWindowPos(HWND_TOPMOST, SWP_SHOWWINDOW |
SWP_NOACTIVATE)` и прячется `ShowWindow(SW_HIDE)`; tao `show()` не вызывается (`ShowWindow(SW_SHOW)`
«Activates the window»). tao считает HUD скрытым — согласованно: флаги tao после создания не
трогаются. Позиция (`hud_rect`): капсула по центру под курсором (над ним у нижнего края), в `rcWork`
монитора курсора; окно 440×112 логических (W2-F: длинные русские фразы с сочетанием) (прозрачное поле под `--shadow-pop`; `lib.rs` создаёт
440×112 = `HUD_SIZE`, размер задаётся и при каждом показе). Порядок: `hud:show` → позиция/размер → показ без
активации → перепроверка геометрии; автоскрытие через 1.66 с после последнего показа (фронт начинает
выход на 1.4 с); повторный показ того же тоста только продлевает его. Кто показывает: вставка —
`copied` (focus_lost/нет цели) и `elevated`; очередь на пути хоткея — `stack_empty`; хоткеи паузы и
пропуска — `paused`/`resumed`/`skip_next`. Фронт — `src/windows/hud/` (капсула 999px не шире 392 px — W3-D: самая длинная фраза, RU «Окно
администратора — нажмите» + «Ctrl+Shift+V», 379 px, раньше резалась до «нажм…»; `--shadow-pop`,
иконка + текст, `{keys}` → чип `Kbd`, вход 190 мс liquid, выход 170 мс, reduced-motion без
движения; превью `/hud.html?kind=copied|elevated|stack_empty|paused|resumed|skip_next`,
`&text=Shift%2BInsert`, `?demo=1`). **`HudShow.text`** (уточнено W2-F, FIX-07) — подпись чипа
`{keys}`: сочетание вставки целевого приложения (§6.6); нет `text` — `hud.pasteKeys` («Ctrl+V»).
У тостов без `{keys}` `text` не показывается (раньше `text` заменял всё сообщение; так его никто
не использовал). `toast.ts::keysLabel` + `tests/hudToast.test.mjs`.

### 7.8. Команды и требования к фронту шелфа (W1-E)

- `picker_hide(restoreFocus)` — Esc (последний слой); `picker_make_interactive` — клик в поиск
  пассивного шелфа; `Err("failed: …")`, если Windows отказал в foreground; второго `picker:open` нет.
- `settings_open(section?)` / `picker::show_settings` — закрыть шелф, показать окно настроек
  (`unminimize`, `show`, `SetForegroundWindow`; отказ → Windows мигает кнопкой), `settings:navigate`.
  При запуске (онбординг, setup шаг 10) webview настроек ещё не подписан — режим онбординга фронт
  берёт из `settings.general.onboardingDone`.
- `monitors_list` — `id` = имя устройства GDI, `name` = EDID-имя монитора (CCD API:
  `QueryDisplayConfig` + `DisplayConfigGetDeviceInfo` GET_SOURCE_NAME/GET_TARGET_NAME), иначе имя
  устройства без `\\.\`; основной первым, затем по расположению.
- Фронт шелфа: в закрытом состоянии и до первого `picker:open` **ничего не рисует** (взведённое окно
  видимо, прозрачно и click-through); `picker:close` → анимация выхода ≤ 250 мс; клик по фону (bleed)
  ничего не делает — закрытие ловит Rust. Повторный `picker:open` без `picker:close` (апгрейд hover →
  summon) — обновить `interactive`/`mode`/`target`, не сбрасывая поиск.
- `picker::on_page_load(app, label)` — повторно шлёт `picker:layout` и (если открыт) `picker:open`
  перезагрузившемуся webview; подключается сниппетом в `lib.rs` (`Builder::on_page_load`).

## 8. Хоткеи (`hotkeys/`)

> Замер: 2026-11-26 (W1-D: реализовано; нативно не запускалось). W3-A: маска после проглоченной
> клавиши при Alt/Win, `main_vk`; нативно не запускалось. W4-A (2026-11-26): сторож LL-хука —
> переустановка каждые 10 с (FIX-12); нативно не запускалось. W6-A (2026-11-27): 9 слотов — пять
> действий скриншотов (§18.11); нативно не запускалось.

Перенос движка Liquid Tools `src-tauri/src/hotkeys.rs` **без** диктовки, PrintScreen-спецслучая,
Esc-перехвата, кнопок мыши, «голых» модификаторов и `crate::Feature`. Файлы: `combo.rs` (нотация +
план привязок, чистая), `native.rs` (поток-регистратор, LL-хук), `mod.rs` (API §15, диспетчер),
`commands.rs`.

```rust
enum HotkeyAction { Summon, StackNext, PauseToggle, IgnoreNext,                  // слоты = поля hotkeys.* по порядку
                    ShotRegion, ShotScreen, ShotWindow, ShotRepeat, ShotText }   // W6-A
// combo::SLOTS = 9, Slots<T> = [T; SLOTS], combo::FIELDS — "hotkeys.summon" … "hotkeys.shotText"
```

- **Нотация** `"Alt + V"` (модификаторы Ctrl · Alt · Shift · Super, затем одна клавиша) ⇄ VK-коды
  физических клавиш — русская раскладка не мешает. Каждый токен `src/lib/hotkey.ts::keyToken`
  разбирается (тест), каждая нативно записанная комбинация разбирается обратно (тест). Отклоняются
  (`invalid`): кнопки мыши (`MMB/M4/M5`), одиночный модификатор (`LCtrl` срабатывал бы на каждом
  Ctrl+C). Односторонний модификатор допустим главной клавишей вместе с другим (`Ctrl + LWin`).
- **Бэкенды.** [`RegisterHotKey`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerhotkey)
  (`hWnd = NULL` → `WM_HOTKEY` в очередь потока-регистратора, всегда `MOD_NOREPEAT`) — по умолчанию.
  `WH_KEYBOARD_LL` — только для клавиш, которые нельзя/не следует регистрировать: Win, Apps, F12
  («reserved for use by the debugger»), односторонний модификатор главной клавишей; хук **ставится,
  только пока такая привязка задана или идёт запись**, и снимается после.
- **Инвариант хука** ([LowLevelKeyboardProc](https://learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc)):
  коллбек только `try_lock` (никогда не ждёт), обновляет атомарную маску модификаторов, решает
  «проглотить» и кладёт задачу в канал единственного потока-диспетчера; IPC, окна, I/O — в
  диспетчере. Синтетический ввод Liqlip (`dwExtraInfo == 0x4C514C50`, маска VK `0xE8` — W1-C
  `paste::input`) пропускается не глядя. Действие идёт на своём коротком потоке (`paste_next`
  блокирует до ~1.6 с).
- **Сторож хука** (W4, FIX-12). Коллбек, не уложившийся в `LowLevelHooksTimeout` (Windows 10 1709+ —
  не больше 1 с), Windows 7+ снимает молча, и, по документации
  [LowLevelKeyboardProc](https://learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc), «there
  is no way for the application to know whether the hook is removed». Надёжного признака снятия нет
  (`SetWindowsHookExW` успешен в момент установки, событий хука при известном нажатии не дождаться;
  что хук снимают сон или блокировка сессии, документация не говорит — `WM_POWERBROADCAST` /
  `WM_WTSSESSION_CHANGE` ловили бы не ту причину, да и окна у потока хука нет), поэтому выбрана
  **переустановка по таймеру**: пока поток хука жив (а он жив ровно пока хук нужен —
  хук-сочетание или запись), таймер потока (`SetTimer(NULL, …)` → `WM_TIMER` в его очередь) каждые
  `HOOK_REFRESH_MS` = 10 с ставит новый хук и только затем снимает старый (`refresh_hook`):
  «SetWindowsHookEx … always installs a hook procedure at the beginning of a hook chain»
  ([Hooks Overview](https://learn.microsoft.com/windows/win32/winmsg/about-hooks)) — мгновения без хука
  нет, а оба вызова идут подряд на потоке хука без прокачки сообщений, так что событие не
  обрабатывается дважды. Не поставился новый — остаётся старый, попытка через 10 с; старый не снялся
  (Windows мог снять его сам) — только лог. После переустановки маска модификаторов пересобирается из
  `GetAsyncKeyState` (`seed_modifiers`): переход, пропущенный снятым хуком, не залипает. Снятый хук
  возвращается в пределах ~10 с (это и есть цена решения: до переустановки хук-сочетание молчит;
  `RegisterHotKey`-сочетания — `Alt + V` и др. — от хука не зависят). Коллбек не изменился — сторож работает в цикле сообщений потока хука, не в коллбеке. Тест порядка
  `hook_watchdog_installs_first_then_removes_the_old_hook`.
- **Применение настроек** (`apply_settings`, синхронно из `settings_set`): `combo::desired` — поле не
  разбирается или повторяет чужое сочетание → `invalid` и прежнее сочетание (сначала «застолбляют»
  неизменённые поля, затем изменённые по порядку); `combo::reconcile` на потоке-регистраторе —
  сначала снимаются все меняющиеся привязки (обмен двух сочетаний работает), затем регистрируются
  новые; отказ ОС (`ERROR_HOTKEY_ALREADY_REGISTERED` → `taken`, иное → `invalid`) возвращает поле к
  прежнему сочетанию, а если оно теперь занято другим полем — тот тоже откатывается (`taken`). Итог
  всегда совпадает с тем, что сохраняет `settings_set`.
- **Конфликт при запуске** (Liquid Tools держит `Alt + V`): сочетание остаётся желаемым, повтор
  регистрации каждые 30 с (`SetTimer` потока) — когда Liquid Tools закроется, `Alt + V` заработает
  сам. `apply_settings` и при неизменных хоткеях (после одной попытки) сообщает такие поля как
  `taken` — для этого `settings_set` должен вызывать хук всегда (сниппет `settings.rs` в отчёте W1-D).
- **Запись** (`hotkey_record_start/stop` → `hotkeys:captured {combo}` в окно настроек): все
  глобальные хоткеи приостановлены (`UnregisterHotKey`), хук ставится; Esc без модификаторов →
  `combo: ""`; аккорд из модификаторов (`Ctrl + LWin`) — на отпускании; записанная клавиша
  проглатывается; одиночный модификатор не записывается; таймаут 30 с → `combo: ""` и возврат
  хоткеев; сохранение изменённых хоткеев (`settings_set`) тоже завершает запись. Хук не ставится →
  `Err("failed: hook: …")`, хоткеи всё равно приостановлены до `stop` (webview-запись
  `hotkeyCapture.ts` ловит обычные аккорды). Фронту настроек: `startHotkeyCapture` без
  `bareModifier`/`mouse` (движок их отклоняет).
- **Действия**: `Summon → picker::summon`; `StackNext → paste::stack::paste_next` (HUD пустой
  очереди / `copied` / `elevated` показывает сама вставка); `PauseToggle → capture::toggle_pause`
  (HUD `paused`/`resumed`); `IgnoreNext → capture::ignore_next` (HUD `skip_next`); `Shot* →
  screenshot::shot_capture { mode }` через `tauri::async_runtime::block_on` на потоке действия (не на
  воркере рантайма; команда сразу возвращается — захват идёт на своём потоке, §18.3).
- **Ошибки для `shot_status`** (W6-A): `hotkeys::current_errors(app)` — ошибки разбора/дубли текущих
  настроек + сочетания, которые ОС не отдала (после одной попытки), по всем 9 полям; W6-B берёт поля
  `hotkeys.shot*`.
- **Маска после проглоченной клавиши** (W3-A). Хук глотает главную клавишу (`Alt + F12`,
  `Super + Menu`) или записанную (`Super + X` в записи) — тогда зажатые Alt/Win отпустились бы
  «голыми»: у Alt — режим меню окна, у Win — «Пуск». Если среди удерживаемых модификаторов есть
  Alt/Win (`combo::masks_release`), коллбек кладёт в очередь диспетчера `Job::Mask` перед действием, и
  диспетчер за доли миллисекунды нажимает неназначенную `0xE8` (`paste::mask_modifier_release`, тег
  `LQLP` — хук её пропускает) — задолго до того, как пользователь отпустит модификатор. Коллбек
  по-прежнему ничего не делает сам (правило 3). Главная клавиша — сам модификатор (`Ctrl + LWin`) —
  маски не требует: её нажатие и отпускание проглочены оба.
- **`hotkeys::main_vk(combo)`** (W3-A, аддитивно) — главная клавиша сочетания тем же разбором; её ждёт
  вставка из очереди (§6.5).

## 9. IPC-контракт

TS — `src/lib/ipc/types.ts`; Rust — `src-tauri/src/model.rs` (`#[serde(rename_all = "camelCase")]`).
Время — **миллисекунды Unix** (`number`/`i64`). Id — `number`/`i64`.

### 9.1. Перечисления

```ts
export type Kind = 'text' | 'link' | 'code' | 'color' | 'contact' | 'image' | 'file' | 'screenshot'; // screenshot — W5 (§18.5)
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
  dir?: string;                                      // file: каталог первого файла (хвост пути; W3, FIX-08)
  shot?: ShotMeta;                                   // screenshot / правленая image (W5, §9.10)
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
  folders: Record<number, number> }   // kinds.screenshot — W5 («Counts += screenshot» §18.10)
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
  hotkeys: { summon: string; stackNext: string; pauseToggle: string; ignoreNext: string;   // "" = не задан
             shotRegion: string; shotScreen: string; shotWindow: string; shotRepeat: string; shotText: string }; // W5
  storage: { retention: '1d' | '1w' | '1m' | '3m' | '6m' | '1y' | 'forever';
             maxEntries: number; maxSizeMb: number;          // 0 = без ограничения
             types: Record<Kind, boolean> };                 // + screenshot (W5)
  privacy: { respectMarkers: boolean; ignoredApps: string[]; secretPatterns: boolean };
  stack: { newlineAfterPaste: boolean };
  ocr: { enabled: boolean; language: 'auto' | string };
  window: { settingsBounds: { x: number; y: number; w: number; h: number } | null };
  screenshot: ScreenshotSettings;                            // W5, §9.10
}
export interface SettingsResult { settings: Settings; errors: { field: string; code: 'taken' | 'invalid' }[] }
```

Дефолты: language system · theme system · autostart **true** · position center · centerRow false ·
monitor cursor · hoverOpen false · hints true · favoritesFirst false · clickAction paste ·
closeOnCopy true · alwaysPlain false · bumpOnPaste false · rememberSearch 60s ·
mergeSeparator newline · pasteKeys [] · hotkeys summon `"Alt + V"`, stackNext `"Alt + Shift + V"`,
остальные `""` · retention 3m · maxEntries 5000 · maxSizeMb 1024 · все типы true ·
respectMarkers true · ignoredApps `[keepass.exe, keepassxc.exe, 1password.exe, bitwarden.exe]` ·
secretPatterns true · newlineAfterPaste false · ocr enabled true, language auto. W5: shotRegion
`"PrtSc"`, shotScreen `"Ctrl + PrtSc"`, остальные три `""`; `types.screenshot` true; `screenshot` —
§9.10.

### 9.5. Прочие типы

```ts
export interface TargetInfo { exe: string; name: string; title: string; iconUrl: string | null; elevated: boolean }
export interface PasteRequest { ids: number[]; mode: PasteMode; separator?: string; keepOpen?: boolean }
export interface TextPasteRequest { text: string; plain: boolean; caretBack?: number; keepOpen?: boolean }
export interface PasteOutcome { result: 'pasted' | 'copied' | 'failed';
  reason?: 'no_target' | 'elevated' | 'focus_lost' | 'clipboard_busy' | 'not_found' | 'stack_empty' | 'partial' }
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
| `snippet_move` | `id, beforeId: number\|null` → `()` (ручной порядок сниппетов, как `folder_move`; W2-B, аддитивно) | store |
| `apps_list` | → `AppInfo[]` (источники с числом записей) | store |
| `storage_stats` | → `StorageStats` | store |
| `clip_paste` | `PasteRequest` → `PasteOutcome` | paste |
| `clip_copy` | `{ ids, mode, separator? }` → `PasteOutcome` | paste |
| `text_paste` | `TextPasteRequest` → `PasteOutcome` | paste |
| `text_copy` | `text` → `()` | paste |
| `clipboard_text` | → `string\|null` (текущий `CF_UNICODETEXT`, для `{clipboard}` в сниппетах; W3, FIX-09) | paste |
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
| `backup_export` / `backup_import` | → `string\|null` / `mode: 'replace'\|'merge'` → `ImportReport` (отмена диалога: экспорт → `null`, восстановление → `invalid: cancelled`; §17.6) | import (W2) |
| `shot_capture` | `mode: ShotMode, delay?: number` → `()` (`region`/`text` — оверлей; `screen`/`window`/`repeat` — сразу коммит; `delay` — обратный отсчёт, с) | screenshot · session (W6-A) |
| `shot_session` | → `ShotSession \| null` | screenshot · session (W6-A) |
| `shot_stage` | `sessionId, stage: ShotStage` → `()` | screenshot · session (W6-A) |
| `shot_commit` | **сырое тело** §18.4 (`[u32 LE N][JSON ShotCommit][RGBA \| ничего]`) → `ShotCommitResult` | screenshot · commit (W6-A) |
| `shot_cancel` | `sessionId` → `()` | screenshot · session (W6-A) |
| `shot_edit` | `id` → `()` (открыть окно редактора) | screenshot · editor (W6-B) |
| `shot_editor_get` | → `ShotEditorPayload \| null` | screenshot · editor (W6-B) |
| `shot_editor_switch` | `accept: boolean` → `()` | screenshot · editor (W6-B) |
| `shot_editor_commit` | **сырое тело** (JSON `ShotEditorCommit`) → `Clip` | screenshot · commit (W6-A) + editor (W6-B) |
| `shot_pin` | `id` → `()` | screenshot · pin (W6-B) |
| `shot_pin_get` | → `PinPayload \| null` (по метке вызывающего окна) | screenshot · pin (W6-B) |
| `shot_pin_resize` | `scale: number` → `()` | screenshot · pin (W6-B) |
| `shot_pin_close` | → `()` | screenshot · pin (W6-B) |
| `shot_save` | `id` → `string` (путь в папке снимков) | screenshot · files (W6-A) |
| `shot_save_as` | `id` → `string \| null` (диалог) | screenshot · files (W6-A) |
| `shot_reveal` | `id` → `()` (Проводник с выделением сохранённого файла) | screenshot · files (W6-A) |
| `shots_hud_layout` | `height: number` → `()` (логические px стопки QAO) | screenshot · qao (W6-B) |
| `shots_hud_hide` | → `()` | screenshot · qao (W6-B) |
| `shot_status` | → `ShotStatus` | screenshot · status (W6-B) |
| `open_windows_settings` | `page: WindowsSettingsPage` → `()` (`keyboard` = `ms-settings:easeofaccess-keyboard`) | screenshot · status (W6-B) |

Ошибки команд — `Result<T, String>` с машинным кодом в начале строки (`"not_found: …"`,
`"busy: …"`, `"invalid: …"`, `"failed: …"` — сбой ОС/плагина, добавлен в W0; заглушки —
`"unimplemented: <команда>"`); фронт показывает локализованный текст по коду
(`src/lib/ipc/errors.ts` → `common.error.<код>`, неизвестный код → `common.error.unknown`).

### 9.7. События (`emit_to` адресно; ⚑ = во все окна)

| Событие | Payload | Кто шлёт → кому |
|---|---|---|
| `clips:changed` | `{ origin: ChangeOrigin; change: 'added'\|'updated'\|'deleted'\|'reset'; ids?: number[] }` | store/capture → picker, settings, shots, editor (W5) |
| `picker:open` | `{ mode: 'summon'\|'hover'; target: TargetInfo \| null; interactive: boolean }` | picker → picker |
| `picker:close` | `{}` | picker → picker (освободить строки через 260 мс) |
| `picker:layout` | `{ position; centerRow; width; height; bleed }` | picker → picker |
| `capture:status` | `CaptureStatus` | capture → picker, settings |
| `stack:changed` | `Stack` | paste → picker |
| `settings:changed` ⚑ | `Settings` | settings → все |
| `settings:navigate` | `{ section: string }` | picker (`settings_open`) → settings |
| `hotkeys:captured` | `{ combo: string }` | hotkeys → settings |
| `hud:show` | `{ kind: 'copied'\|'stack_empty'\|'paused'\|'elevated'\|'resumed'\|'skip_next'\|'countdown'\|'shot_failed'\|'text_copied'\|'text_empty'; text?: string }` (`resumed`/`skip_next` — W1-D, аддитивно; `text` — подпись чипа сочетания `{keys}`, напр. «Shift+Insert», W2-F; нет — «Ctrl+V»; W5: `countdown` — `text` = оставшиеся секунды в том же чипе, `shot_failed` / `text_copied` / `text_empty` — §18.3–18.4) | picker/paste/hotkeys/screenshot → hud |
| `ocr:progress` | `{ done: number; total: number }` | ocr → settings |
| `import:progress` | `{ phase: string; done: number; total: number }` | import → settings |
| `theme://changed` ⚑ | `'light'\|'dark'` | theme → все |
| `shot:session` | `ShotSession` | screenshot → capture (начало сессии, смена монитора) — W5 |
| `shot:end` | `ShotEnd { sessionId }` | screenshot → capture, shots — W5 |
| `shot:added` | `ShotAdded { clip: Clip\|null; savedPath; tempPath; monitor; error: 'save_failed'\|null }` | screenshot → shots — W5 |
| `shot:editor` | `ShotEditorPayload` | screenshot → editor — W5 |

**Фронт отличает свои изменения от чужих по `origin`:** плашка «Новые записи» — только для
`origin: 'capture'`.

### 9.8. URI-схема `liq`

`register_asynchronous_uri_scheme_protocol("liq", …)` (по образцу `ltthumb` Liquid Tools,
`lib.rs:198–226`); в WebView2 адрес `http://liq.localhost/<path>`:
`/thumb/<id>?v=<n>` · `/media/<id>?v=<n>` · `/icon/<appId>?v=<n>` (версия у всех трёх — W1-A: после
пересоздания базы id могут совпасть, и без версии WebView2 показал бы чужую картинку из кэша) —
чтение файла в `spawn_blocking`, 404 для отсутствующих.

**Кэш** (W4, FIX-14; `store::cache_control(path)`, тест): `/thumb` и `/media` — `Cache-Control:
no-store`: это содержимое буфера, и превью удалённой (в том числе «Удалить недавнее», приватной)
картинки не должно оседать в дисковом HTTP-кэше WebView2 (`%LOCALAPPDATA%\com.liqlip.app\EBWebView`).
`/icon` — `public, max-age=31536000, immutable` (иконки приложений — не содержимое, версия в query);
всё неизвестное — `no-store`. Лишних запросов это не добавляет: в пределах страницы Chromium
переиспользует уже загруженную картинку, а сетка шелфа виртуализирована (в DOM только экран ± 300 px),
и карточка, пересозданная при прокрутке назад, получает новый `<img>` с тем же URL. Проверено в Chromium
156 (встроенный браузер, тот же движок Blink, что у WebView2) на тестовом сервере с `no-store`: 300
разных картинок загружены, удалены из DOM, через 5 с созданы снова — 0 повторных запросов, все `<img>`
`complete` сразу (HTML: «list of available images» — переиспользование в документе, даже когда HTTP
кэшировать не разрешает). Заново картинка запрашивается только новой страницей (перезагрузка окна) —
это и есть цель `no-store`. Id записей не переиспользуются (`AUTOINCREMENT`), так что удалённую картинку
в памяти страницы новая запись не «унаследует».

**Скриншоты (W5, §18.3).** Замыкание схемы в `lib.rs` отдаёт пути `screenshot::owns_uri` (`/shot/…`,
`/base/…`) в `screenshot::serve_uri(app, path) -> Option<(mime, bytes)>` (заглушка W5 — всегда `None`
→ 404; наполняет W6-A в `screenshot/frame.rs`), остальное — `store::serve_uri`, как было. Ответ тот же:
`Cache-Control` из `store::cache_control` (для `/shot` и `/base` — `no-store`, тест
`screenshot::tests::liq_routes_and_their_cache_policy`), ACAO `*`.
`/shot/<sessionId>/<monitorIndex>.bmp` — кадр монитора сессии; `/base/<clipId>?v=<n>` — исходник
картинки для редактора.

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
- **`PasteReason` + `partial`** (W1-C): часть записей пропущена — картинки при склейке текста, не-файлы при вставке файлов; вставлено остальное.
- **`PasteReason` + `stack_empty`** (координатор, приёмка W0): `stack_paste_next`/`stack_paste_all` при пустой очереди → `{ result: 'failed', reason: 'stack_empty' }` (хоткей дополнительно показывает `hud::show(StackEmpty)`).
- **Безопасные заглушки** (до W1 возвращают `Ok`): `capture_status` (запись идёт), `stack_get`
  (пусто), `picker_target` (`null`). **Реализованы в W0:** `settings_get`, `settings_set`, `app_info`,
  `open_path`, `pick_exe`. Остальные 57 → `Err("unimplemented: <имя>")`.
- **`settings_set`**: семантически неверные поля сохраняют прежнее значение и попадают в `errors`
  (`invalid`); типовой мусор (не тот JSON-тип, неизвестное значение enum) отклоняется Tauri до вызова
  (ошибка команды). Хоткеи — синхронный хук `hotkeys::apply_settings` (§15): отвергнутое поле
  возвращается к прежнему. `autostart` применяется при изменении (в любой сборке) и сверяется на
  старте (только release — debug-сборка не прописывает dev-бинарник в автозапуск). Успешное
  изменение: атомарная (W3: и надёжная — `sync_all` перед переименованием) запись `settings.json` →
  замок записи отпускается (W3, §15 правило 7) → подписчики → `settings:changed` ⚑ → при смене темы
  `theme://changed` ⚑ (разрешённая тема). Загрузка `settings.json` — по полям: неверное поле → дефолт.
  Дубль в `privacy.ignoredApps` / `picker.pasteKeys` (ручная правка файла) при загрузке схлопывается —
  остаётся первое вхождение без учёта регистра и пробелов по краям, список не откатывается к умолчаниям
  (W4, FIX-17); такой файл считается «не чистым» — оригинал сохраняется в `settings.json.invalid`.
  `settings_set` список с дублями по-прежнему отвергает (`invalid`).
- **Тема.** `theme://changed` несёт разрешённую тему (`'light'|'dark'`); шлётся при смене
  `general.theme` и при смене темы Windows, если выбрано `system`.

### 9.10. Скриншоты (W5-A, 2026-11-26)

Контракт §18.10 внесён в код аддитивно (`model.rs` ≡ `types.ts`, раздел «§9.10 Screenshots»), версия
настроек остаётся 1. Типы — как в §18.10; здесь — то, что §18.10 не задавал, и где код уточняет его.

- **Именованные типы для inline-форм §18.10:** `ShotMeta` (`ClipMeta.shot`), `ShotSessionMode`
  (`'region'|'text'`), `ShotStage`, `ShotEditorAction`, `ShotAddedError`, `WindowsSettingsPage`,
  `ScreenshotSettings` + `ShotFormat` / `ShotAfterCapture` / `ShotCorner`, payload событий `ShotEnd`,
  `ShotAdded`. `ShotStatus.hotkeyErrors` — `SettingsError[]` (та же форма `{field, code}`).
- **`Rect`** — `{x, y, w, h}` числами (Rust `f64`): оверлей переводит координаты указателя и может
  прислать дробные; коммит округляет и зажимает в кадр (W6-A, `clamp_crop`). `WinRect` и
  `MonitorFrame` — целые физ. px (их отдаёт Rust).
- **`AnnotDoc` / `AnnotObject`** (§18.6) — только в TS; в Rust поле `doc` — `serde_json::Value`
  (`Option` там, где `AnnotDoc | null`), Rust документ хранит и передаёт, не разбирая.
  `tests/contract.test.mjs` держит `AnnotDoc` в списке TS-only.
- **`Counts += screenshot`** — это `Counts.kinds.screenshot`: `kinds` — `Record<Kind, number>`, ключ
  появился вместе с `Kind`. Отдельного поля верхнего уровня нет. Так же `storage.types.screenshot`.
  Пилюля «Картинки» в шелфе уже считает `image + screenshot` (`pills.ts`).
- **`ScreenshotSettings`** (дефолты §18.10, фикстура `tests/fixtures/settings.default.json`): `quick`
  false · `magnifier` true · `dimensions` true · `sound` true · `copy` true · `save` true · `folder` ""
  · `fileName` "Liqlip {date} {time}" · `format` png · `jpgQuality` 90 · `afterCapture` overlay ·
  `corner` left · `timeout` 10. Проверка (`settings::validate` ≡ мок): `folder` ≤ 260 символов без
  управляющих; `fileName` непустой, ≤ 120, без `< > : " / \ | ? *` (зарезервированы в именах файлов
  Windows, learn.microsoft.com «Naming Files, Paths, and Namespaces»); `jpgQuality` 60–100; `timeout`
  0–3600 (UI предлагает 0/5/10/30/60). Пять `hotkeys.shot*` — длина ≤ 64, без управляющих (как
  остальные), `revert_hotkey` знает их поля.
- **Старый `settings.json`** (1.0) грузится без потерь — тест `settings::tests::a_v1_0_file_upgrades_without_loss`
  на фикстуре `tests/fixtures/settings.v1.0.json`. Новые поля: `#[serde(default)]` (`Settings.screenshot`,
  контейнерный default у `ScreenshotSettings`, `default = …` у пяти хоткеев). Вложенный объект
  `storage.types` ленивый загрузчик (`parse_lenient`) накладывает **по ключам** на дефолт
  (`overlay_nested`): старые тумблеры остаются, `screenshot` получает `true`. Мок (`localStorage`)
  сливает так же.
- **Хоткеи** `shot*` хранятся и проверяются, но **не регистрируются** до W6-A (движок знает 4 слота,
  `hotkeys::combo::FIELDS`); тест-хелпер `combo::tests::hk` заполняет их пустыми. Окно настроек
  показывает 4 глобальных действия (`HOTKEY_ACTIONS`) и группу «Скриншоты» из 5 слотов
  (`SHOT_HOTKEY_ACTIONS`, W6-F, §18.5 «UI»); дубль проверяется по всем 9.
- **Аргументы команд (ключи JS):** `shot_capture {mode, delay}` (`delay` можно `null`/не передавать),
  `shot_stage {sessionId, stage}`, `shot_cancel {sessionId}`, `shot_edit/shot_pin/shot_save/
  shot_save_as/shot_reveal {id}`, `shot_editor_switch {accept}`, `shot_pin_resize {scale}`,
  `shots_hud_layout {height}`, `open_windows_settings {page}`. `shot_pin_get/resize/close` получают
  вызывающее окно (`tauri::WebviewWindow` — аргумент команды, JS его не передаёт).
- **Сырое тело** (`shot_commit`, `shot_editor_commit`): TS — `frameBody(header, rgba?)`
  (`src/lib/ipc/frame.ts`) → `invoke(cmd, Uint8Array)` (`@tauri-apps/api` 2.11: `InvokeArgs` принимает
  `Uint8Array`; v2.tauri.app «Calling Rust» → raw request); заголовки не нужны — всё в теле. Rust —
  `screenshot/body.rs`: `Request::body()` → `InvokeBody::Raw` (tauri 2.11.2); запасной путь
  `postMessage` присылает те же байты JSON-массивом чисел — тоже принимается. Любое битое тело —
  `invalid: …` (короткий префикс, пустой/обрезанный/не тот JSON, пиксели при 0×0, не
  `width*height*4`, сторона > 32 768, заголовок > 64 МБ); тесты `screenshot::body::tests` ≡
  `tests/shotFrame.test.mjs`. Делегату приходит заголовок и пиксели срезом тела (`Option<&[u8]>`).
- **Все команды screenshot — `async`** (не главный поток: синхронная команда Tauri идёт на главном
  потоке — v2.tauri.app «Calling Rust», Async Commands): построение окна из `shot_edit`/`shot_pin` — по
  документированному безопасному пути, и ни одна команда не ждёт замок, который держит поток,
  ждущий главный поток.
- **Заглушки W5:** каждая команда — делегат в функцию своего подмодуля (`screenshot/{session,commit,
  files,editor,pin,qao,status}.rs`), каждая возвращает `Err("unimplemented: <команда>")`; `frame::serve`
  → `None` (404); `session::init`, `windows::init`, `session::on_capture_close`,
  `editor::after_commit` — пустые. Полный список сигнатур — §18.10 «Заглушки».
- **`Kind::is_image()` / `isImageKind(kind)`** — «картинка» = `image | screenshot`. W5 перевёл на них
  ветки вставки/копирования/drag-out/открытия/OCR (`paste/{compose,dragout,open,stack}.rs`,
  `ocr/mod.rs`), хэш (`content.rs`: семейство `image` — дедуп снимка с той же картинкой), шелф
  (карточка, Quick Look, меню, `preview.ts`, пилюля «Картинки»), мок. **Не тронуто (W6-A):** `store/*`
  (в т. ч. SQL `kind = 'image'` в `store/{ocr,media}.rs`), `import/backup.rs`.

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
  `query.ts` (токены → `FeedQuery`), `layout.ts`, `gridNav.ts`, `transforms.ts`, развёртка сниппетов — `src/lib/snippets/expand.ts` + `overlays/snippetText.ts`,
  `format.ts`/`preview.ts`.
  Как сделано в W1-E (компоненты, состояние, клавиатура, точки расширения для W2-C, мок шелфа,
  «Замер») — реверс-док [`picker.md`](picker.md).
- **Настройки** (`windows/settings/`): `Settings.svelte` — сайдбар + секции (`sections/*.svelte`),
  онбординг (`onboarding/*.svelte`); стор настроек — один `settings.svelte.ts` поверх
  `settings_get/set` + `settings:changed`.
- **Окно настроек (W1-F, сделано)** — реверс-док [settings.md](settings.md): корень `SettingsWindow.svelte` (коллизия `Settings.svelte` ↔ `settings.svelte.ts` на регистронезависимой ФС), разделы, онбординг, общая развёртка плейсхолдеров `src/lib/snippets/expand.ts`.
- **Тема/язык** — из `Settings.general` (бэкенд авторитетен; рассылка `settings:changed`).

## 11. Дизайн-система

**Канон — [`design-system.md`](design-system.md) (W2-E, 2026-11-26)**: токены `src/app.css`,
шкалы радиусов/отступов/высот, правило концентричности (пары `--r-*`/`--pad-*`, вложенный радиус
через `calc()`), примитивы и матрица состояний; аудит «до/после» — [`design-audit.md`](design-audit.md).
Идентичность Liquid Tools сохранена: монохром-лок (цвет — только пользовательский контент),
сквирклы (`squircle` = `corner-shape: squircle`) у поверхностей, капсулы `999px` у контролов,
Inter + JetBrains Mono (моно — функционально), моушен 150–250 мс, жидкая кривая,
`prefers-reduced-motion`. Окно настроек — те же токены на системном заголовке.

**Что изменилось относительно Liquid Tools (W2-E):**
- заливки контролов (`track · hover · wash-sel · selected · toggle-track`) — **смывки ink**
  (`color-mix`), а не непрозрачные серые: контрол отличим от любой подложки в обеих темах;
  алиасы поверхностей в корне окна настроек удалены;
- новые семантические поверхности: `ground · tile · raised · card · card-hover · card-lift`, плюс
  `scrim`, `thumb`; синеватые серые нейтрализованы;
- панель шелфа r28 / padding 12 для всех расположений; карточка — одна плитка r16 (превью 4:3 +
  мета 32 внутри), текущая — тон и подъём вместо кольца; кнопки, поля, дропдауны — капсулы;
- `:focus-visible` больше не сбрасывает радиус в 4 px — кольцо концентрично элементу;
- скроллбары без стрелок (`::-webkit-scrollbar`), края с продолжением затухают маской;
- `--text-micro` 9.5 → 11 px; мета и подписи ≥ 12 px.

**Перенос W0 (2026-11-26, история).** `app.css` — токены 1:1 из Liquid Tools; выкинуты
`.studio-shot-corners`, `--gradient-avatar`, `--dur-cap-morph`, `body.liquid-main`. Компоненты
`src/lib/components/` — копии Liquid Tools (`Ico`, `Button`, `Toggle`, `SegmentedControl`,
`Dropdown` (bits-ui), `MenuItem`, `Skeleton`, `EmptyState`, `Toast`, `FlashPill`, `Kbd`, `Badge`,
`SearchField`, `BigTypeList`); `Menu` очищен от вкладок/аккаунта; захардкоженные `aria-label`
заменены ключами `common.*`. Иконки — 75 Heroicons outline (`@iconify-json/heroicons` 1.2.3,
stroke 1.5, `1em`), реестр `src/lib/icons.ts`, лицензия `public/licenses/heroicons.txt`. В W2-E
компоненты переведены на шкалы и капсулы; `Toast` получил `action`, `Toggle`/`SegmentedControl`/
`Dropdown` — `id` и `describedby`.

**W3-D (2026-11-26).** Неиспользуемые переносы удалены: `Menu`, `MenuItem`, `FlashPill`, `BigTypeList`,
`flash.svelte.ts`, `hotkeyError.ts` (и их ключи `common.hotkey.*`). Новый общий примитив — `use:scrollFade`
(`src/lib/scrollFade.ts` + `[data-fade]` в `app.css`): вертикальный скроллер затухает краем, за которым
есть содержимое (design-system §7). `t()` подставляет `{слоты}` за один проход (`i18n/format.ts`): значение
не сканируется повторно, ``Dropdown` — `id` и `describedby`.
`/`$1` в нём — не шаблоны замены.

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

**W5 (скриншоты, фундамент).** Node: `shotFrame` (кадрирование тела ≡ Rust `body.rs`), `shotsMock`
(мок скриншотов: сессия и фальшивый рабочий стол, коммит → запись + события, редактор, закрепление,
файлы, статус); `contract` считает 87 команд и держит `AnnotDoc` TS-only; `ocrMock` — очередь OCR
охватывает и `screenshot`. Rust: `screenshot::body::tests` (5), `screenshot::tests` (маршруты `liq`),
`settings::tests::{a_v1_0_file_upgrades_without_loss, screenshot_fields_are_validated}` (фикстура
`tests/fixtures/settings.v1.0.json`), `model::tests::screenshot_wire_shapes`,
`paste::compose::tests::screenshot_composes_like_an_image`.

**W6-A (нативный захват, 2026-11-27).** Rust, +42 (394 всего): `screenshot::frame` (BMP-заголовок
читается `image` обратно, кроп BGRA → RGBA, `clamp_crop` LT, округление `Rect`, маршруты),
`screenshot::grab` (монитор точки / ближайший, порядок, список мониторов Win32 без чтения экрана),
`screenshot::targets` (клип, дерево и предел 32 уровней, рендереры первыми, порядок бюджета детей,
предел узлов), `screenshot::session` (одна сессия за раз и токены, payload по монитору, этапы, кроп
окна, `--capture`), `screenshot::commit` (план действий, 64 Мп, картинка/исходник/документ,
миниатюра), `screenshot::files` (шаблон, `{n}` / ` (2)`, атомарное имя во временной папке, JPG поверх
белого, папка, даты UTC), `store::shots` (вставка, аннотированный снимок и удаление, дедуп и вид,
первая правка `image`, `saved_path`, сироты `media\base`, импорт документов, валидация),
`store::schema::v1_database_migrates_to_v2`, `store::ocr::screenshots_are_queued_with_images`,
`hotkeys` (9 слотов), `tray` (пункты скриншотов), `import::tests::screenshots_keep_their_documents_through_a_backup`.
Сам захват GDI, окна, фокус и `PrtSc` в тестах не трогаются (нужен живой рабочий стол) — чек-лист
отчёта W6-A.

**W6-B (окна, статус, импорт снимков LT).** Rust, только чистая логика и временные папки (окна не
создаются): `screenshot::windows::tests` (выбор монитора, логические → физические px, сторож главного
потока), `screenshot::qao::tests` (угол рабочей области на 100 % и 150 %, потолок высоты, удаляются только
файлы прямо в каталоге temp), `screenshot::editor::tests` (имя записи, закрытие после `done`/смены,
размер окна 80 %, PNG), `screenshot::pin::tests` (размер 1:1 и 80 %, центр у курсора, ресайз вокруг
курсора, предел 32 767), `screenshot::status::tests` (умолчание «Ножниц» по KB5027303, фильтр `hotkeys.shot*`,
чтение реестра машины — только чтение), `import::lt::tests` (строка снимка → `screenshot`, BMP → PNG,
пропуски: нет файла / вне кэша / `..` / относительный / формат / папка; счёт импорта) + 1 `#[ignore]` до
W6-A (`screenshots_land_as_screenshot_records_once`). `import::tests` — ожидания с учётом строки снимка
вне кэша в фикстуре (20 записей пробника, `skipped` + 1).

**W6-E (QAO и закреплённые окна).** Node: `shotsStack` (6: порядок, вытеснение, повтор записи,
удаление, ключи, таймеры и паузы, геометрия окна), `shotsQao` (18: высота окна и её задержка,
пауза ховером/drag/сессией/скрытой страницей, `timeout 0`, жизненный цикл drag-out — `dropped`,
`cancelled`, ошибка, устаревший drag, `pointercancel`, клик; кнопки и их ошибки, OCR, `clip: null`,
`clips:changed`), `pinMath` (8: колесо, масштаб и его пределы, непрозрачность, 100 % ↔ исходный,
масштаб окна, радиус, форма и место меню).

## 13. Владение файлами и правила для агентов

- **Файлы-узлы координатора:** `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`,
  `src-tauri/capabilities/*.json`, `src-tauri/src/model.rs` + `src/lib/ipc/types.ts` (контракт),
  `src/lib/i18n/{en,ru}.ts` (агенты дописывают только свой префикс), `src/app.css`.
  С W0 к ним относятся и общие модули: `src-tauri/src/{settings,content,events,tray,theme,
  i18n_native,logx,fsx,autostart}.rs`, `src/lib/ipc/{commands,events,runtime,errors}.ts`,
  `src/lib/ipc/mock/bus.ts`, `src/lib/{appearance,theme.svelte,hotkey,hotkeyCapture,scrollFade,browserKeys}.ts`,
  `src/lib/components/**`, `tests/{contract,settingsDefaults,ipcMock}.test.mjs`,
  `tests/helpers/`, `tests/fixtures/`. Нужна правка — сниппет в отчёте.
- **Скриншоты (W5–W6):** владение зонами — **§18.1**. С W5 к узлам добавлены
  `src-tauri/src/screenshot/{mod,body}.rs` (команды-делегаты, состояние, маршруты `liq`, кадрирование
  тела), `src/lib/ipc/frame.ts`, `src/lib/ipc/mock/shots.ts`, `capabilities/screenshot.json`,
  `capture.html` · `shots.html` · `editor.html` · `pin.html`, `vite.config.ts`. Подмодули
  `screenshot/{grab,targets,session,frame,commit,files}.rs` — W6-A, `{windows,qao,editor,pin,status}.rs`
  — W6-B: каждая зона держит своё состояние в своём файле (`ScreenshotState` хранит `Mutex<зона::State>`),
  так что `mod.rs` им править не нужно.
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
7. **Настройки: никто не уведомляется под замком записи** (W3, находка W3-A). `settings_set` в два шага:
   `commit` под `SettingsStore.write` — валидация, синхронные побочные эффекты (хоткеи, автозапуск),
   атомарная запись, публикация в `current`, номер коммита; замок **отпускается**; `announce` под
   отдельным замком уведомлений — подписчики, `settings:changed`, тема (`theme::emit_changed` ждёт
   главный поток). Порядок уведомлений = порядок коммитов: изменение старше уже объявленного не
   объявляется (объявленное шло от состояния до обоих к состоянию после обоих — подписчик видит каждое
   поле и никогда не видит откат). Подписчику можно читать `settings::get`, ждать главный поток, писать
   геометрию окна; нельзя звать `settings_set` (ждал бы сам себя). Под замком уведомлений никто не
   берёт замок записи. Тест — `settings::tests::subscribers_run_without_the_write_lock`,
   `announcements_never_go_backwards`.

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
//   вызывается после сохранения, до settings:changed, БЕЗ замка записи настроек (правило 7, W3);
//   old = последнее объявленное состояние; НЕ вызывать settings_set из подписчика
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
// W5: pub fn shot_session(app, &ShotSession); shot_end(app, session_id: i64); shot_added(app, &ShotAdded);
//     shot_editor(app, &ShotEditorPayload); метки CAPTURE = "capture", SHOTS = "shots", EDITOR = "editor",
//     PIN_PREFIX = "pin-"

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
pub fn cache_control(path: &str) -> &'static str;            // её Cache-Control, §9.8 (W4, FIX-14)

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

**Реализовано W1-A (2026-11-26)** — сигнатуры выше не менялись; поведение (подробно — §4.2–4.6):
- `insert_clip` нормализует `NewClip`: текст > 2 МБ и список > 2 000 путей режет и **перехэширует**
  функциями `content.rs`; html/rtf > 2 МБ отбрасывает; ширину/высоту картинки берёт из заголовка PNG;
  пустое превью строит сам; метрики, если захват их не дал, — `content::text_metrics`. **`size`
  пересчитывается** из реально сохранённого (text + все `clip_formats`, включая полный PNG) — поле
  `NewClip.size` справочное. Ошибки: `invalid: …` (пустой текст, картинка без PNG / > 64 МП, файлы без
  путей, пустой хэш), `busy`/`failed` (БД, диск). Подъём существующей: `copied_at = max(…)`,
  `copy_count + 1`, воскрешение, источник и `kind` — новые, `meta` — слияние. После — сигнал ретенции.
- `set_app_icon`: PNG (сигнатура) ≤ 1 МБ → `icons\<id>.png`; нет приложения → `not_found`.
- `get_for_paste`: несуществующая, мягко или жёстко удалённая → `not_found: clip <id>` (как и все
  обращения по id: `clip_detail`, `clip_rename`, `clip_update_text`, `clips_delete`, избранное, папка;
  на этом префиксе вставка чистит очередь); html — UTF-8 `String`, rtf — байты, files — из
  `clip_formats` (или из `text`); `png_path` — **абсолютный** путь (`Paths` приводит каталог данных к
  абсолютному) и только если файл на месте (иначе и `formats` без `png`).
- `mark_used`: неизвестные/удалённые id пропускаются; `bump` → `copied_at = max(copied_at, now)`.
- `delete_recent`: число удалённых; мягко удалённые в окне тоже; вызывающий эмитит `deleted` (трей —
  без `ids`, команда — с `ids`).
- `init`: фоновый поток `liqlip-store` + `settings::subscribe` (только сигнал на смену `storage.*`).
- Команды store эмитят `clips:changed {origin:'user'}` сами, после успеха; правки папок и сниппетов —
  `{change:'updated'}` без `ids` (единственный межоконный сигнал: пилюли папок и счётчик сниппетов
  должны обновиться, если правка была в другом окне).

**capture (W1-B)** — `src-tauri/src/capture/{mod,classify}.rs`

```rust
pub fn start(app: &AppHandle);                                    // setup шаг 5: listener + worker
pub fn mark_own_write(seq: u32);                                  // ← paste: GetClipboardSequenceNumber после записи
pub fn status(app: &AppHandle) -> CaptureStatus;                  // ← трей, capture_status
pub fn pause(app: &AppHandle, minutes: Option<i64>) -> Result<CaptureStatus, String>; // ← трей
pub fn resume(app: &AppHandle) -> Result<CaptureStatus, String>;  // ← трей
pub fn toggle_pause(app: &AppHandle) -> Result<CaptureStatus, String>; // ← хоткей PauseToggle
pub fn ignore_next(app: &AppHandle) -> Result<CaptureStatus, String>;  // ← трей, хоткей IgnoreNext
pub fn cancel_ignore_next(app: &AppHandle) -> Result<CaptureStatus, String>; // ← трей: повторный клик по отмеченному пункту (W2-F)
pub fn app_icon_data_url(exe: &str) -> Option<String>;           // ← paste (TargetInfo.iconUrl); не блокирует: промах → None + фоновое извлечение
pub mod classify { pub fn classify_text(text: &str) -> (Kind, ClipMeta); }   // ← store
// добавлено W1-B (аддитивно):
pub fn stop();                                                     // ← lib.rs RunEvent::Exit: снять listener, остановить потоки
pub fn set_collect(app: &AppHandle, on: bool) -> Result<CaptureStatus, String>; // режим сбора в очередь
pub fn running_apps() -> Vec<RunningApp>;                          // ← apps_running (blocking-пул)
pub fn clipboard_sequence() -> u32;                                // GetClipboardSequenceNumber для mark_own_write
// добавлено W3-A (аддитивно):
pub fn clipboard_text() -> Result<Option<String>, String>;         // ← clipboard_text (FIX-09), блокирует ≤ ≈1.5 с
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
Поток вставки (реализация W1-C, подробно — §6.6): `store.get_for_paste` → запись в буфер (+
`capture::mark_own_write(seq)`) → ждать отпускания клавиш → `SetForegroundWindow(цель)` →
`picker::hide_now` → перепроверка foreground → ввод → `store.mark_used` → `clips:changed
{origin:'user'}`; нет фокуса/elevated → копирование + `picker::hud::show(…)`. Дополнительно для W1-D:
`paste::focus::handoff_active()` и `paste::dragout::active()` — пока `true`, выход шелфа по потере
фокуса не срабатывает.

**picker + hud (W1-D)** — `src-tauri/src/picker/{mod,hud}.rs`

```rust
pub fn init(app: &AppHandle);                          // после создания окон: subscribe(picker.*), hover-поллер
pub fn summon(app: &AppHandle);                        // ← хоткей, трей, single-instance (повтор = закрыть)
pub fn hide_now(app: &AppHandle);                      // ← paste: мгновенно, БЕЗ возврата фокуса
pub fn show_settings(app: &AppHandle, section: Option<&str>); // ← settings_open, трей, онбординг (setup шаг 10)
pub mod hud { pub fn show(app: &AppHandle, kind: HudKind, text: Option<String>); } // ← paste, хоткеи
pub fn on_page_load(app: &AppHandle, label: &str);     // W1-D, аддитивно: ← lib.rs Builder::on_page_load (повтор layout/open)
pub fn close(app: &AppHandle, restore_focus: bool);    // W3-A, аддитивно: ← paste (копия без цели), picker_hide — закрытие как Esc
```
**W3-A, аддитивно:** `hotkeys::main_vk(combo) -> Option<u16>` (← `paste::stack`), `paste::mask_modifier_release()`
(← диспетчер хоткеев, §8).
Выход по потере фокуса читает `paste::focus::handoff_active()` / `paste::dragout::active()` (W1-C) —
после них шелф становится пассивным, а не закрывается (§7.2).

**hotkeys (W1-D)** — `src-tauri/src/hotkeys/mod.rs`

```rust
pub enum HotkeyAction { Summon, StackNext, PauseToggle, IgnoreNext }
pub fn start(app: &AppHandle, hotkeys: &HotkeySettings) -> Vec<SettingsError>;   // setup шаг 7
pub fn apply_settings(app: &AppHandle, prev: &HotkeySettings, next: &HotkeySettings) -> Vec<SettingsError>;
//   ← settings_set (синхронно); field "hotkeys.<имя>", code taken|invalid; движок держит прежнюю привязку
```
Действия: `Summon → picker::summon`, `StackNext → paste::stack::paste_next` (не в коллбеке хука),
`PauseToggle → capture::toggle_pause`, `IgnoreNext → capture::ignore_next`.

**ocr (W2-A)** — `pub fn start(app: &AppHandle)` (setup шаг 5, после `store::init`) + команды; аддитивно `pub fn wake(app: &AppHandle)` — разбудить очередь и поставить в неё картинки, ни разу не стоявшие в очереди (для импорта). Внутри хранилища — `store/ocr.rs` (§5a.2). **import (W2-B)** — только
команды; методы `Store` для импорта и восстановления — `store/bulk.rs` (`pub(crate) mod bulk`,
§17.7); после импорта и восстановления зовёт `ocr::wake`.

**screenshot (W5 → W6-A/W6-B)** — `src-tauri/src/screenshot/mod.rs` (узел) и заглушки подмодулей
(полный список — §18.10 «Заглушки»):

```rust
pub struct ScreenshotState { capture: Mutex<session::SessionState>, qao: Mutex<qao::QaoState>,
                             editor: Mutex<editor::EditorState>, pins: Mutex<pin::PinState> } // manage() в шаге 5
pub fn init(app: &AppHandle);                           // шаг 5: session::init + windows::init
pub fn on_capture_close(app: &AppHandle);               // ← lib.rs CloseRequested окна capture → session::on_capture_close
pub fn owns_uri(path: &str) -> bool;                    // `/shot/…` | `/base/…`
pub fn serve_uri(app: &AppHandle, path: &str) -> Option<(String, Vec<u8>)>;   // → frame::serve (404 до W6-A)
pub(crate) mod windows { pub(crate) fn exclude_from_capture(win: &WebviewWindow) -> Result<(), String>; } // WDA_EXCLUDEFROMCAPTURE
// model.rs: Kind::is_image(self) -> bool  (image | screenshot)
```

**W6-B → W6-A** (оконная сторона, которую зовут сессия и коммит; любой поток, не блокируют; §18.8–18.9):

```rust
screenshot::qao::show_added(app: &AppHandle, payload: ShotAdded);   // вместо events::shot_added (окно строится при первом)
screenshot::qao::hide_for_capture(app: &AppHandle);                 // начало любой сессии (и без оверлея)
screenshot::qao::restore_after_capture(app: &AppHandle);            // конец сессии: коммит или отмена
screenshot::editor::open(app: &AppHandle, id: i64) -> Result<(), String>;   // afterCapture: editor
screenshot::pin::open(app: &AppHandle, id: i64) -> Result<(), String>;      // action: pin
// W6-A → W6-B (швы `editor::seam`, подключаются на INT-5): store::shots::doc, store::shots::apply_edit,
// paste::copy_png, files: перезапись saved_path, files: «Сохранить как…» для PNG-байт.
```

**Фронт.** Окна общаются с Rust только через `src/lib/ipc/commands.ts` (`call` → `invoke` или мок) и
`events.ts` (`on*` → `listen` или мок-шина). Тема/язык окна — `initAppearance()` из `appearance.ts`
(`settings_get` + `settings:changed`, плюс `theme://changed`). Общая развёртка плейсхолдеров —
`src/lib/snippets/` (W1-F). Мок: `mock/index.ts` (роутер, `MOCK_COMMANDS`), `bus.ts` (шина событий,
флаги `?mock-empty=1`, `?mock-error=1`, `?mock-live=0`, `?mock-size=N`,
`?mock-target=chrome|none|elevated`, `?mock-unimpl=ocr,import`; задержка 30–80 мс), стабильный API
`settings.ts::currentSettings()` для остальных мок-модулей.

## 16. Упаковка (W2-D, 2026-11-26)

> Замер: 2026-11-26 (W2-D: знак, иконки, трей, NSIS 1.0.0, CSP, release-профиль, лицензии;
> установщик собран, не запускался — PQ-02). W2-F: в трее отмечается взведённое «Пропустить
> следующее» (FIX-05, §16.3) — проверено тестами подсказки, меню нативно не открывалось. W3-A:
> `tray::refresh` не ждёт главный поток (§16.3, взаимоблокировка с `settings_set`).

### 16.1. Знак

**Идея:** буфер обмена + жидкость — родственник колбы Liquid Tools. Портретный планшет-сквиркл,
капсула-зажим поверх верхней кромки, внутри жидкость с рябью. В паузе жидкость уступает место паре
капсул «‖» в том же планшете. Монохром: без градиентов, теней и свечения.

**Геометрия:**
- углы планшета — квартика `|x|⁴ + |y|⁴ = 1`, то же, что CSS `corner-shape: squircle`
  (`superellipse(2)`): одна кубическая кривая на угол, ручки на 9,09 % радиуса от угла;
- зажим — капсула с круглыми концами (правило дизайн-системы: капсулы — не сквиркл);
- радиусы концентричны: внутренний = внешний − отступ;
- на крупных размерах вокруг зажима вырезан зазор (маска `liqlip-clip-gap`), и зажим читается
  отдельной деталью.

**Один генератор — `node scripts/brand.mjs`.** Геометрия живёт в коде: `MASTER` — сетка 24,
`TRAY` — по пиксельным размерам. Растр делает `tauri icon` проекта (resvg), без новых зависимостей.
Скрипт перезаписывает всё ниже; после него — `cargo test` (тест трея сверяет размеры PNG).

| Файл | Что |
|---|---|
| `design/logo.svg` · `design/logo-1024.png` | исходник знака (чёрный, viewBox 24, 1024 px) |
| `design/app-icon.svg` · `design/app-icon-1024.png` | плитка приложения: сквиркл `#171717` (тёмный `paper`) со знаком `#F4F5F7` (тёмный `ink`), как у Liquid Tools; знак — 68 % плитки, радиус — 36 % стороны |
| `public/brand/logo-light.svg` · `logo-dark.svg` | знак для интерфейса: `ink` светлой (`#252525`) и тёмной (`#F4F5F7`) темы, viewBox 24; от 24 px (мельче — глифы трея) |
| `src-tauri/icons/icon.ico` + `32x32` · `64x64` · `128x128` · `128x128@2x` · `icon.png` | иконки приложения (`tauri icon design/app-icon.svg`) |
| `src-tauri/icons/tray/<px>.png` · `<px>-paused.png` | глифы трея 16/20/24/28/32/40/48 px, чёрные на прозрачном |
| `src-tauri/icons/nsis-sidebar.bmp` | картинка страниц «Добро пожаловать»/«Готово» установщика, 164×314, 24-bit BMP |

**Оптические размеры трея.** 16/20/24 px нарисованы по пиксельной сетке: кромки и обводка — на
целых или половинных пикселях. Зазора между стенкой и жидкостью нет, зажим слит с кромкой, волна
одна, S-образная (рябь на 16 px не видна). С 28 px — полный знак (`MASTER` в масштабе).

### 16.2. Иконки приложения

`tauri icon` (v2.tauri.app/develop/icons) рендерит SVG под каждый размер. ICO содержит слои
32, 16, 24, 48, 64, 256 (PNG); 32 стоит первым, как требуют доки. macOS-, AppX- и мобильные файлы
генератор выкидывает: Liqlip только для Windows.

В `bundle.icon` четыре файла: 32, 128, 128@2x и ico. Windows берёт ico для ресурса exe, иконки окна
и ярлыков. Мелкие слои ICO (16/24) — тот же вектор без хинтинга, поэтому они мягче глифов трея.

### 16.3. Трей (`tray.rs`)

**Иконка.** Размер — `SM_CXSMICON` на DPI панели задач: `GetDpiForWindow(Shell_TrayWnd)`, иначе
`GetDpiForSystem`. Получается 16 px при 100 %, 20 — при 125 %, 24 — при 150 % и так далее. Берётся
PNG ближайшего размера не меньше нужного; оболочке не приходится масштабировать картинку. Глиф
чёрный, для тёмной панели (`SystemUsesLightTheme = 0`) он перекрашивается в белый с сохранением
альфы. Пауза — свой глиф «‖».

**Подсказка.** Формат: «Liqlip — запись идёт» / «Liqlip — пауза до 14:30» / «Liqlip — запись на
паузе», пока взведено «Пропустить следующее» — ещё « · Следующее копирование не сохранится»
(FIX-05, W2-F), а пока очередь не пуста — « · Очередь: 3». Время переводится в локальный пояс через
`SystemTimeToTzSpecificLocalTime` и форматируется `GetTimeFormatEx` в формате пользователя. Строки —
`i18n_native` (`TrayTooltipPausedUntil`, `TrayTooltipSkipNext`); самая длинная подсказка < 128
UTF-16 (предел `szTip`, тест).

**Меню** сверено со SPEC §5. «Пропустить следующее копирование» — check-пункт Tauri
(`CheckMenuItem`, FIX-05): отмечен, пока пропуск взведён (`CaptureStatus.ignoreNext`); клик по
отмеченному снимает пропуск (`capture::cancel_ignore_next`), по неотмеченному — взводит. muda сам
переключает галочку при клике, `refresh` затем строит пункт из настоящего состояния; решение
принимается по текущему статусу, а не по увиденной галочке. Когда пропуск срабатывает (захват
тратит его) или снимается, `capture` зовёт `publish` → `tray::refresh` — отметка исчезает.

**Скриншоты** (W6-A, SPEC §9.8 + пункты «трей» из §9.1), сразу после «Открыть Liqlip»: «Снимок
области» · «Снимок экрана» · «Снимок активного окна» · «Повторить прошлую область» · «Снимок с
задержкой ▸ 3 / 5 / 10 с» (область после отсчёта) · «Скопировать текст с экрана». Справа — сочетание из
`hotkeys.shot*` (через таб, как у «Открыть»; не задано — без подсказки). Клик (главный поток) →
короткий поток ждёт 250 мс, пока меню растворится (иначе оно попадёт в кадр), и зовёт
`screenshot::shot_capture { mode, delay }`. Ключи `i18n_native::TrayShot*`. Меню перестраивается при
смене любого хоткея (`old.hotkeys != new.hotkeys`).

**Обновление.** `refresh` вызывается при смене паузы, пропуска, очереди, языка, хоткея и темы. На смену
масштаба на лету отдельного вызова нет (сниппет для `lib.rs` — в отчёте W2-D); иначе иконка
обновится при ближайшем `refresh`.

**`refresh` никогда не ждёт** (W3-A). Каждый вызов трея/меню Tauri (`set_menu`, `set_icon`,
`MenuItem::with_id`…) делает круговой рейс на главный поток и ждёт ответа без таймаута
(`run_main_thread!`). Из потока, держащего замок, который нужен главному, это взаимоблокировка:
`settings_set` зовёт подписчиков под замком настроек (смена языка/хоткея → `refresh`). Поэтому
перестройка всегда идёт **на главном потоке**: там — сразу, из других потоков — задачей
`run_on_main_thread`, вызывающий возвращается немедленно. В очереди не больше одной перестройки
(флаг снимается в начале задачи, до чтения состояния): поток изменений (режим сбора, 10 копий/с)
перестраивает меню один раз и ничего не теряет. Заодно захват, вставка и команды больше не стоят,
пока главный поток занят (модальный цикл drag-out).

### 16.4. Установщик (NSIS, `tauri.conf.json`)

| Ключ | Значение | Зачем |
|---|---|---|
| `productName` / `version` | `Liqlip` / `1.0.0` (так же в `Cargo.toml` и `package.json`) | `app_info.version` = `package_info().version` = версия из конфига |
| `bundle.targets` | `["nsis"]` | только `…-setup.exe` |
| `bundle.publisher` / `copyright` | `Liqlip` / `© 2026 prodbyEDDY. MIT License.` | «Приложения» Windows, свойства exe, строка внизу окон установщика |
| `bundle.shortDescription` / `longDescription` | EN | описание exe и пакета |
| `nsis.installMode` | `currentUser` | без прав администратора, `%LOCALAPPDATA%\Liqlip`, ключи в HKCU |
| `nsis.languages` | `["English", "Russian"]`, `displayLanguageSelector: false` | язык по языку системы, иначе первый (английский); строки Tauri для русского есть (`Russian.nsh`) |
| `nsis.installerIcon` / `uninstallerIcon` | `icons/icon.ico` | вместо иконок NSIS по умолчанию |
| `nsis.sidebarImage` | `icons/nsis-sidebar.bmp` | вместо синей картинки NSIS по умолчанию |

**Сознательно не задано:**
- `headerImage`: в MUI2 картинка шапки стоит слева и сдвигает заголовок страницы на 150 px; без неё
  шапка чище.
- `startMenuFolder`: ярлык ложится прямо в «Пуск» (`Programs\Liqlip.lnk`).
- `webviewInstallMode`: остаётся по умолчанию `downloadBootstrapper` (тихий). WebView2 на
  Windows 10/11 обычно уже есть; если нет, установщик скачает бутстрэппер, нужен интернет.
- Подпись и автообновление (PQ-03): SmartScreen покажет «Windows защитила ваш компьютер» →
  «Подробнее → Выполнить в любом случае».

**Поведение шаблона Tauri:**
- ярлык на рабочем столе — галочкой на последней странице;
- «Запустить Liqlip» — там же, запуск от имени пользователя;
- повторная установка поверх — обновление, данные сохраняются; `allowDowngrades` по умолчанию `true`;
- установщик просит закрыть запущенный Liqlip.

**Удаление** — через «Параметры → Приложения» или `uninstall.exe`:
- убирает файлы, ярлыки и значение автозапуска `HKCU\…\Run\Liqlip` — его имя совпадает с
  `productName`, так значение называет `tauri-plugin-autostart`;
- галочка «Удалить данные приложения» дополнительно удаляет `%APPDATA%\com.liqlip.app` (база,
  медиа, настройки, логи) и `%LOCALAPPDATA%\com.liqlip.app` (данные WebView2).

### 16.5. CSP

```json
"csp": {
  "default-src": "'self'",
  "connect-src": "ipc: http://ipc.localhost 'self'",
  "img-src": "'self' data: http://liq.localhost",
  "style-src": "'self' 'unsafe-inline'",
  "font-src": "'self' data:"
}
```

**Что дописывает Tauri** (`tauri-codegen`/`tauri` 2.11, `manager::set_csp`):
- в `script-src` — `'self'` и sha256 каждого JS из `dist`;
- в `style-src` — nonce только для элементов `<style>` в HTML. В наших HTML их нет, поэтому
  `'unsafe-inline'` не отменяется nonce'ом.

**Директивы:**
- `connect-src`: IPC Tauri v2 — `fetch` на `http://ipc.localhost` (`ipc:` — на других ОС), как в
  доках Security → CSP. `'self'` — для будущего встроенного просмотра лицензий (`fetch` своих
  `/licenses/*.txt`).
- `img-src`: `http://liq.localhost` — схема `liq` в WebView2 (§9.8); `data:` — иконки приложений и
  мок-превью.
- `font-src data:`: Vite встраивает мелкий сабсет JetBrains Mono (< 4 КБ) как `data:` (`assetsInlineLimit`).
- `style-src 'unsafe-inline'` нужен. Проверено в Chromium (тот же движок, что WebView2) на `dist` под
  этим заголовком:
  - без `'unsafe-inline'` блокируются `style`-атрибуты разметки (`<body style="background:
    transparent">` в `picker.html`/`hud.html` — шелф и HUD потеряли бы прозрачность) и
    `setAttribute('style')`;
  - CSSOM (`el.style.*`, `style.cssText`) работает и без него;
  - с итоговой политикой нарушений нет, шрифты Inter и JetBrains Mono грузятся.

**Где действует.** Только в собранном приложении: в `tauri dev` WebView грузит Vite напрямую, и
заголовок CSP не ставится. Проверка — на установленной сборке. Откат — `"csp": null`, одна строка.

### 16.6. Release-профиль (`Cargo.toml`)

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

**`opt-level = 3`**, а не `"s"`, который советуют доки Tauri (App Size): «use 3 if you prefer
speed». Горячие места Liqlip — это:
- SQLite: `cc` собирает его с тем же уровнем;
- кодек PNG для картинок буфера;
- FTS-поиск.

Для личного установщика размер не критичен. Выигрыш размера от `"s"` не замерялся: разрешена одна
release-сборка.

**`lto`, один codegen unit и `strip`** возвращают большую часть размера.

**`panic` не задан**, остаётся `unwind`: захват перезапускает свои потоки через `catch_unwind`
(§5), а `abort` сделал бы это невозможным.

### 16.7. Лицензии

**Собственная лицензия Liqlip** — MIT с 2026-11-26 (решение Eddy: открытый бесплатный проект,
ROADMAP §11). Где: `LICENSE` в корне; его копия для окна «О Liqlip» — `public/licenses/LICENSE.txt`;
`bundle.copyright`; поле `license` в `Cargo.toml` и `package.json`. Правообладатель — `prodbyEDDY`.
До этого v1 была проприетарной («все права защищены»).

**Сторонние компоненты** — `node scripts/licenses.mjs` → `public/licenses/THIRD-PARTY.txt`.
Запускать после изменения зависимостей; вывод детерминирован. Что попадает в список:
- **Rust:** `cargo metadata --filter-platform x86_64-pc-windows-msvc --locked --offline`, обход
  обычных (normal) зависимостей от `liqlip`. Build/dev-зависимости и proc-macro-крейты пропускаются
  вместе со всем, что достижимо только через них. `cargo metadata` объединяет фичи, поэтому
  несколько крейтов, нужных только `tauri-build`, всё же попадают в список — это безвредно.
- **npm:** `pnpm licenses list --json --prod` (в том числе рантайм Svelte как peer-зависимость
  `bits-ui`) плюс `tailwindcss`: его preflight копируется в CSS. Набор сверен с sourcemap
  production-сборки.

**Формат файла:**
- у каждого пакета — лицензия и ссылка;
- тексты лицензий из самого пакета; для «A OR B» берётся один (MIT, если есть);
- одинаковые тексты печатаются один раз;
- пакеты без файла лицензии вынесены отдельным списком с лицензией из метаданных.

На 2026-11-26: 234 крейта + 36 npm-пакетов, 131 уникальный текст, 285 КБ.

**Где лежит.** Vite копирует `public/licenses/` в `dist`, и Tauri вшивает файлы в exe. Раздел «О
Liqlip» показывает все три текста (`LICENSE.txt`, `THIRD-PARTY.txt`, `heroicons.txt`) внутри окна.

### 16.8. Как собрать установщик

```bash
pnpm install
node scripts/brand.mjs        # только после правки знака
node scripts/licenses.mjs     # после изменения зависимостей
CARGO_TARGET_DIR=… pnpm tauri build   # beforeBuildCommand = pnpm build
```

**Результат:** `<target>/release/bundle/nsis/Liqlip_1.0.0_x64-setup.exe`. NSIS и плагин
`nsis_tauri_utils` CLI Tauri при первой сборке сам скачивает в `%LOCALAPPDATA%\tauri\NSIS`.

**В `dist` не попадают:** `.map` (sourcemap включается только при `TAURI_ENV_DEBUG`), тесты и
фикстуры.

**Попадает:** мок-адаптер `mock-*.js` — до 33 КБ с демо-данными. В Tauri он не загружается:
`import("./mock")` выполняется только вне Tauri.

**Замер сборки W2-D** (2026-11-26, первая release-сборка в общем target):
- `pnpm tauri build` — 219 с, из них cargo release — 3 мин 25 с;
- `liqlip.exe` — 15 175 168 байт (14,5 МиБ);
- `Liqlip_1.0.0_x64-setup.exe` — 4 219 963 байт (4,0 МиБ);
- единственное предупреждение сборщика — идентификатор `com.liqlip.app` оканчивается на `.app` (касается только macOS; менять нельзя: это путь к данным).

## 17. Импорт и резервные копии (`import/`, `store/bulk.rs`) — W2-B

> Замер: 2026-11-26 (W2-B: импорт из Liquid Tools, экспорт `.liqlip`, восстановление `replace`/`merge`,
> `snippet_move`; `cargo test` — 289 в крейте, из них 23 теста W2-B на синтетической базе в схеме Liquid
> Tools и во временных хранилищах; реальная база Liquid Tools в тестах не открывалась; нативно не
> запускалось). W2-F (2026-11-26): фильтр «похоже на секрет» в импорте и `merge` (FIX-01), одна
> строка `apps` на импорт и захват (FIX-02), восстановление настроек по полям (FIX-03); `cargo test`
> — 300 в крейте, из них 8 новых тестов W2-F; только временные данные. W3-B (2026-11-26): фильтр
> игнорируемых приложений в импорте и `merge`, `meta.dir` у файлов, проверка формы базы копии,
> ретенция приостановлена на время `replace`, восстановление прерванного `replace` при старте,
> поколение базы для OCR; `cargo test` 334 в крейте; только временные данные. W4-A (2026-11-26):
> правило игнор-листа — одна функция захвата, копия в `runner.rs` удалена (FIX-16). W6-B (2026-11-27):
> снимки Liquid Tools → `screenshot` (файл только из кэша LT, нет файла — `skipped`); тесты на синтетической
> базе и поддельном кэше во временной папке; запись в стор ждёт W6-A (`store/bulk.rs`).

Файлы: `import/lt.rs` (чтение Liquid Tools) · `import/backup.rs` (zip: экспорт, проверка, распаковка,
`replace`, `merge`) · `import/runner.rs` (общий цикл импорта) · `import/tmp.rs` (временные каталоги с
guard-ом) · `import/commands.rs` (диалоги, `import:progress`, события, «занято») · `store/bulk.rs`
(методы `Store`). Подключение в хранилище — одна строка `pub(crate) mod bulk;`.

### 17.1. Импорт из Liquid Tools

**Источник** — `%APPDATA%\com.liquidtools.app\clipboard\clipboard.db` (`PathResolver::data_dir()` +
идентификатор) и файлы `<id>.png` рядом (Liquid Tools `docs/library.md` §4, `clipboard/store.rs`).

**Только чтение, без влияния на работающий Liquid Tools.** База никогда не открывается на месте:
она копируется (вместе с `-journal`/`-wal`, если есть; `-shm` не нужен — SQLite перестроит индекс по
WAL) в `%TEMP%\liqlip-lt-*`, и открывается копия. Причины (отклонение от «открыть `mode=ro`, при
занятости — копия»): (1) любое открытие оригинала, даже `SQLITE_OPEN_READONLY`, берёт разделяемую
блокировку, а у Liquid Tools нет `busy_timeout` — его ближайший INSERT захвата упал бы с
`SQLITE_BUSY`; (2) у WAL-базы read-only соединение создаёт и пишет `-shm` в его каталоге. Копирование
читает файлы обычным `ReadFile` (std открывает с `FILE_SHARE_READ|WRITE|DELETE`), байтовые блокировки
SQLite лежат на смещении 1 ГиБ и чтению не мешают. Запись, попавшая в окно копирования, ловится
сравнением размера/mtime базы и наличия `-journal`/`-wal` до и после — копия повторяется (≤ 4 раз).
Копия без сопутствующих файлов открывается `SQLITE_OPEN_READ_ONLY`; с ними — на запись (это наша
копия: SQLite откатывает «горячий» журнал или проигрывает WAL), затем `PRAGMA quick_check(1)`. Каталог
копии удаляется guard-ом. PNG читаются из каталога Liquid Tools только на чтение. Тесты сверяют
отпечаток каталога (имя, размер, mtime, хэш содержимого каждого файла) до и после — в том числе при
открытой транзакции писателя с журналом и при WAL-базе с открытым писателем.

**Проверка схемы.** Нет заголовка `SQLite format 3` → `invalid: not a Liquid Tools database`; нет
`clipboard_entries` с `id, kind, preview, created_at` → `invalid: unknown Liquid Tools database format`;
`SQLITE_CORRUPT`/`NOTADB`/`quick_check ≠ ok` → `invalid: the Liquid Tools database is damaged`.
Необязательные колонки (`content_ref, source_app, pinned, html, category, app_icon`, таблица `categories`)
определяются через `pragma_table_info` — старые версии Liquid Tools тоже импортируются.

**Отображение схемы Liquid Tools → Liqlip**

| Liquid Tools | Liqlip |
|---|---|
| `kind` text · link · code · contact · color | текстовая запись; вид — заново `capture::classify::classify_text` (все текстовые виды — одно семейство хэша, метка на дедупликацию не влияет; `123456` без `#` больше не «цвет») |
| `kind` image | `image`: `<dir>\<id>.png` (иначе абсолютный `content_ref`), декод в RGBA8 + нормализация как в захвате (полностью прозрачная → непрозрачная), `content::image_hash` по пикселям; PNG копируется как есть (перекодируется, только если нормализация его изменила); миниатюра ≤ 360 px строится сразу. Нет файла → `errors + 1` |
| `kind` file | `file`: пути из `content_ref`/`preview` (через `\n`, ≤ 2000), `meta.ext`, `files` (`dropEffect` 1) |
| `kind` screenshot (W6-B, §18.5) | `screenshot` **без документа аннотаций** (в LT они запечены в картинку), `meta.shot = {savedPath: null, edited: false}`. Файл — `content_ref` (иначе `preview`, LT пишет путь и туда): абсолютный путь, после `canonicalize` (`..`, ссылки) — файл **внутри кэша снимков LT** `%LOCALAPPDATA%\com.liquidtools.app\screenshots` (LT: `app_cache_dir()/screenshots`, `app_cache_dir` на Windows = `FOLDERID_LocalAppData` + идентификатор; путь — `SHGetKnownFolderPath`). Формат — по содержимому (`with_guessed_format`: LT сохранял PNG/JPG/WebP/AVIF); PNG копируется как есть, иное декодируемое сборкой — перекодируется в PNG (в истории только PNG); хэш — `content::image_hash` (семейство `image`: тот же снимок, скопированный картинкой, — одна запись), миниатюра ≤ 360 px. Нет файла, путь вне кэша, относительный путь, папка, формат без декодера в сборке (WebP/AVIF; JPG — после фичи `jpeg` W6-A) → `skipped`; битый файл внутри кэша → `errors + 1`. `source_app`, `pinned`, `category` — как у остальных |
| `kind` любой другой | не переносится и не считается. Диктовки живут в `dictation-history.json`, в этой базе их нет |
| полный текст | `content_ref` (> 280 символов), иначе `preview` (Liquid Tools хранил текст ≤ 280 символов обрезанным по краям пробелов); усечение до 2 МБ (`meta.truncated`); `content_ref` читается как `substr(…, 1, 2 100 000)` — результат тот же, память страницы ограничена |
| `html` (сырой CF_HTML с заголовком) | формат `html` без заголовка (`StartHTML..EndHTML`; без контекста — фрагмент в минимальном документе), `SourceURL` → `meta.url/host`, если у записи нет ссылки |
| `created_at` (мс; < 1e11 — секунды до v3, ×1000, как `migrate_timestamps` Liquid Tools) | `created_at` = `copied_at` |
| `pinned` | избранное; порядок — как в пилюле «Избранное» Liquid Tools (`created_at DESC`), после уже имеющегося избранного Liqlip |
| `category` + таблица `categories` | папки: сначала `categories` по `position`, затем имена, встречающиеся только в записях; совпадение по имени без учёта регистра (Unicode), управляющие символы чистятся, ≤ 64 символа. Переносятся все папки Liquid Tools, в том числе пустые и папки только со снимками |
| `source_app` (имя файла exe без расширения) | приложение `exe = "<stem>.exe"` (нижний регистр), `name = stem`; если в Liqlip уже есть приложение с таким именем файла (полный путь из захвата) — используется оно; а когда захват позже увидит этот exe с полным путём, он не заводит вторую строку, а дописывает путь в эту (`write.rs::match_app`, FIX-02, §4.2) |
| `app_icon` (data-URL PNG; в Liquid Tools есть только у строк снимков) | иконка приложения (по одной на `source_app`), если у приложения её ещё нет |
| `title` | не переносится: в Liquid Tools заголовки только автоматические |
| `size`, `width`, `height`, `thumb`, `ocr_text` | не используются (размер и габариты считаются заново, OCR Liquid Tools удалён) |

**Правила.**
- **Срок хранения.** История (не избранное и не в папке) старше `storage.retention` не переносится —
  первая же ретенция Liqlip удалила бы её. Пробник считает так же, поэтому `imported + skipped + errors =
  probe.clips`. Лимиты `maxEntries`/`maxSizeMb` применяет обычная ретенция после импорта.
- **«Похоже на секрет»** (FIX-01, W2-F). При включённом `privacy.secretPatterns` текстовая запись,
  которая целиком — номер карты по Луну, ключ API или приватный PEM-ключ (`capture::secrets`, то же
  правило, что у захвата, §5), не переносится и считается `skipped`; фраза, в которой ключ лишь
  упомянут, переносится. Фильтр — в общем цикле `runner.rs`, поэтому действует и для `merge` (§17.5).
  Пробник такие записи считает (равенство выше сохраняется). В лог — только число.
- **Игнорируемые приложения** (W3). Запись, источник которой в `privacy.ignoredApps` (KeePass, Bitwarden…),
  не переносится и считается `skipped` — захват такую не сохранил бы (SPEC §2.4.2). Правило одно —
  функция захвата `capture::snapshot::is_ignored` (`pub(crate)`, реэкспорт `capture::is_ignored`;
  W4, FIX-16 — копии в `runner.rs` больше нет): полный путь / имя файла / голая основа без регистра,
  `/` ≡ `\`; exe, известный только по имени (Liquid Tools хранил основу), совпадает и с записью-путём с
  тем же именем файла (захват всегда передаёт полный путь, для него это правило не срабатывает).
  Тоже в общем цикле — действует для `merge`; пробник считает.
- **Повторный импорт** не создаёт дублей и не поднимает записи: совпадение хэша с уже существующей
  записью → `skipped`, её время и счётчики не трогаются; избранное (если не было) и папка (если не было)
  дополняются. Мягко удалённая запись с тем же хэшем не воскрешается.
- **Дубли внутри источника** (Liquid Tools хранил каждое неподряд идущее копирование отдельной строкой)
  сливаются в одну запись этого же прогона: `created_at` — самый ранний, `copied_at` — самый поздний,
  `copy_count` — число строк; в отчёте такие строки — `skipped`.
- Пакеты по 200 строк в порядке `(created_at, id)`, по транзакции на пакет и по savepoint на запись:
  ошибка одной записи → `errors + 1`, импорт идёт дальше; сбой БД (диск, `busy`) прерывает импорт, уже
  записанные пакеты остаются (повторный запуск продолжит без дублей).
- PNG и миниатюры пишутся во временные файлы `media\.staging-*` **до** захвата блокировки БД и
  переименовываются в транзакции (как в захвате); откат записи удаляет её файлы. Иконки приложений
  пишутся после коммита (id откатанного приложения может быть выдан снова).
- Прогресс `import:progress`: `folders` (0/N → N/N), затем `clips` (done/total после каждого пакета).
  В конце — `clips:changed { origin: 'import', change: 'added' }` без `ids` и `ocr::wake` (картинки
  импорта приходят с `ocr_state = none`). `ImportReport.folders` — сколько папок **создано**;
  `ImportProbe.folders` — сколько папок в Liquid Tools.
- `ImportProbe.found = false`, если базы нет **или** переносить нечего (0 записей и 0 папок): шаг
  онбординга не показывается зря. `path` — только при `found`.
- Логи — только числа и коды ошибок, без содержимого.
- **Снимки (W6-B).** `KINDS` включает `screenshot`: пробник считает их, папки «только со снимками» —
  непустые. Строки снимков без пригодного файла не доходят до общего цикла (`runner.rs` не тронут): `lt.rs`
  считает их сам, добавляет в `skipped` и в конце шлёт `import:progress clips total/total` (полоса
  заканчивается полной); равенство `imported + skipped + errors = probe.clips` сохраняется (тест).
  `lt::run` берёт кэш через `FOLDERID_LocalAppData`; `lt::run_in(…, shots: Option<&Path>, …)` — с явным кэшем
  (тесты: поддельный кэш во временной папке; в тестовой сборке `run` реальный профиль не читает — кэш `None`).
  **Зависимость от W6-A:** `store/bulk.rs::normalize` ведёт `Kind::Screenshot` веткой текста
  («`invalid: empty text`») — до правки (`Kind::Image =>` → `k if k.is_image() =>`, проверено локально, в ветку
  не внесено) снимок считается `errors`; тест `screenshots_land_as_screenshot_records_once` помечен `#[ignore]`
  до INT-5. Строку `shot_docs` у импортированного снимка (§18.5: «есть у каждого screenshot») создаёт или не
  создаёт `bulk` W6-A — импорт документ не передаёт.

### 17.2. Формат `.liqlip`

Zip (крейт `zip` 9.8, Deflate через `flate2`/miniz_oxide):

```
manifest.json          Deflate
clipboard.db           Deflate (ZIP64, если ≥ 4 ГиБ) — снимок liqlip.db
settings.json          Deflate — Settings на момент экспорта
media/<id>.png         Stored — PNG записей (картинки и PNG-альтернативы текста)
media/thumbs/<id>.png  Stored — миниатюры картинок и снимков
media/base/<id>.png    Stored — W6-A: картинки без аннотаций (строки shot_docs с has_base = 1)
icons/<id>.png         Stored — иконки приложений
```

W6-A (схема v2): документы аннотаций едут в таблице `shot_docs` внутри `clipboard.db`, исходники — в
`media/base/`; `schemaVersion` новой копии = 2, копию v2 Liqlip 1.0 не примет («made by a newer
Liqlip»), копия v1 восстанавливается в v2 (`replace` мигрирует базу при открытии, в `merge` таблицы
`shot_docs` просто нет).

```json
{ "format": "liqlip-backup", "formatVersion": 1, "appVersion": "0.1.0", "schemaVersion": 1,
  "createdAt": 1795638957418,
  "counts": { "clips": 1240, "favorites": 32, "folders": 3, "snippets": 5, "apps": 14, "media": 300 } }
```

`schemaVersion` = `PRAGMA user_version` базы. Восстановление отказывает с понятной ошибкой, если
`format ≠ "liqlip-backup"` или нет манифеста (`invalid: not a Liqlip backup`), если `formatVersion > 1`
или `schemaVersion` новее сборки (`invalid: the backup was made by a newer Liqlip`), если нет базы
(`invalid: the backup has no database`), если база не открывается, `quick_check ≠ ok` или версия не та
(`invalid: the backup database is damaged`). Счётчики манифеста справочные.

### 17.3. Экспорт (`backup_export`)

1. Системный диалог сохранения (`tauri-plugin-dialog`, родитель — окно настроек, фильтр `*.liqlip`,
   имя `Liqlip-YYYY-MM-DD.liqlip` по локальной дате `GetLocalTime`); отмена → `null`; имя без
   расширения получает `.liqlip`.
2. Снимок без остановки приложения (`Store::snapshot_to`): `VACUUM INTO` на втором соединении
   `SQLITE_OPEN_READ_ONLY` (WAL: захват и UI продолжают работать) во `%TEMP%\liqlip-backup-*`; затем в
   копии `foreign_keys = ON`, `secure_delete = ON`, удаление мягко удалённых записей (с их форматами и
   FTS-токенами), `VACUUM`, `journal_mode = DELETE` — один самодостаточный файл без следов удалённого.
3. Из снимка — списки id с PNG, картинок и приложений с иконкой; файлы берутся из `media\`, `icons\`
   (удалённый за это время файл пропускается).
4. Zip пишется в `<цель>.<pid>.part` рядом с целью, `sync_all`, затем переименование в цель (замена
   существующей). `.part` и временный каталог удаляются на любом пути.
5. Прогресс: `clips` 0/1 → 1/1 (снимок), `media` (упаковано / всего).

### 17.4. Восстановление `replace`

1. Диалог открытия (фильтр `*.liqlip`); отмена → **`Err("invalid: cancelled")`** (выбрано вместо отчёта
   с нулями: пустая копия и отмена иначе неотличимы; UI должен молча игнорировать эту ошибку).
2. Манифест (§17.2) → распаковка в `<data>\restore-staging-*` (тот же том, что и данные: каждый шаг
   подмены — `rename`; остатки прерванного восстановления удаляются в начале). Распаковываются только
   точные имена (`clipboard.db`, `media/<цифры>.png`, `media/thumbs/<цифры>.png`, `icons/<цифры>.png`,
   `settings.json`) — запись за пределы каталога невозможна; размер записи ограничен (PNG ≤ 512 МБ),
   CRC-32 проверяет zip-ридер.
3. Проверка распакованной базы (§17.2) и `count(*)` записей. W3: не только версия, но и **форма** —
   все колонки, которые читает хранилище, `clips_fts`/`snippets_fts` и шесть их триггеров; иначе чужой
   файл с `user_version = 1` подменял базу, и приложение падало на каждом запросе.
4. `Store::replace_with` под мьютексом соединения: соединение закрывается (на его месте — пустая
   in-memory заглушка, её никто не видит), текущие `liqlip.db`, `-wal`, `-shm`, `media\`, `icons\`
   переезжают в `backup-before-restore-<ms>\` (переименования с повторами до ~1,5 с: файл может быть
   открыт схемой `liq` на мгновение), на их место — распакованные, `create_dirs`, открытие обычным
   `schema::open_or_recover` (WAL, миграции, ремонт `fav_order`), сверка числа записей (иначе испорченный
   файл был бы молча пересоздан пустым). Успех → новое соединение, книга undo очищается, старые
   `backup-before-restore-*` удаляются (остаётся одна, последняя), сигнал ретенции. Поле `ocr`
   (`OcrHook`, W2-A) не трогается — меняется только соединение; поколение базы растёт (W3, §5a.2).
   **Сбой посередине** (W3): до первого переноса пишется (с `sync_all`) маркер `restore.pending` с
   именем каталога `backup-before-restore-<ms>`; успех и чистый откат его удаляют. Найденный при старте
   (`bulk::recover_interrupted_restore`, из `Store::open` до открытия базы) — процесс умер посреди
   подмены: всё, что уже лежит в отодвинутом каталоге, возвращается на место (база — вместе со своими
   `-wal`/`-shm`, сайдкары восстановленной базы удаляются; затем `media`, `icons`), маркер и пустой
   каталог удаляются. Раньше приложение стартовало с пустой или наполовину восстановленной базой, а
   прежние данные лежали в каталоге, который удалило бы следующее восстановление. Остатки
   `restore-staging-*` удаляются при каждом старте.
5. **Откат** при сбое любого шага: удаляются положенные на место файлы (и `*.corrupt-<ms>`, если
   `open_or_recover` успел их отодвинуть), отодвинутые данные возвращаются, старая база открывается
   снова, каталог `backup-before-restore-<ms>` удаляется; ошибка `failed: …`. Если вернуть не удалось —
   ошибка называет каталог, где лежат прежние данные (они не удаляются), а маркер остаётся — следующий
   старт повторит возврат.
6. **Ретенция приостановлена** (W3, `RetentionHold` в `import/commands.rs` → `Store::hold_retention`)
   с начала команды до применения настроек копии; снятие запускает отложенный проход уже с ними.
   Раньше сигнал ретенции из `replace_with` срабатывал сразу, с текущими (например, дефолтными
   на новом компьютере) лимитами, и удалял восстановленную историю сверх них раньше, чем
   применялись настройки копии.
7. После успеха (в команде): очередь вставки очищается (её id указывали на прежнюю базу),
   `settings.json` копии разбирается по полям так же, как `settings.json` при старте
   (`settings::parse_file`, FIX-03, W2-F: миграция версии, поле, которое не подходит, — и поле,
   которого в файле нет, — получает значение по умолчанию, все остальные восстанавливаются; раньше
   одно битое поле отбрасывало весь файл). Сохраняются геометрия окна настроек и пройденный
   онбординг. Файл, который вообще не JSON-объект, игнорируется с записью в лог (иначе он сбросил бы
   всё в дефолты). Результат применяется через `settings::settings_set` (валидация, хоткеи,
   автозапуск, `settings:changed`), затем `clips:changed { origin: 'import', change: 'reset' }` и
   `ocr::wake`. Отчёт: `imported` — живых записей, `folders` — папок, `skipped = errors = 0`.

### 17.5. Восстановление `merge`

Распаковка во `%TEMP%\liqlip-merge-*`, проверка базы, затем живые записи копии (`deleted_at IS NULL`,
тот же фильтр срока хранения, что у Liquid Tools) идут **через тот же цикл импорта** (`runner.rs` →
`Store::import_batch`): дедупликация по хэшу копии (тот же алгоритм `content.rs`), существующие записи не
поднимаются (избранное и папка дополняются), медиа и миниатюры копируются, форматы (html, rtf, files,
PNG-альтернатива), заголовок, счётчики, времена, OCR-текст и состояние переносятся, приложения — по
`exe` с тем же сопоставлением, что у захвата (`write.rs::match_app`: полный путь копии дописывается в
строку, известную только по имени файла, — FIX-02; с иконкой, если у приложения её нет), тексты,
похожие на секрет, при `privacy.secretPatterns` пропускаются (FIX-01), папки сливаются по имени без учёта регистра,
избранное — в порядке копии после имеющегося. Потом сниппеты: такой же (имя + тело) пропускается, иначе
добавляется в конец, ключевое слово — только если свободно. Настройки не трогаются. Прогресс: `media`
(распаковка), `folders`, `clips`. В конце — `clips:changed { origin: 'import', change: 'added' }` и
`ocr::wake`. W6-A: после записей — строки `shot_docs` копии (с тем же фильтром) пачками по 50
(`backup.rs::merge_shot_docs` → `Store::import_shot_docs`): запись находится по хэшу (новый id), строка
добавляется, только если у записи её ещё нет, исходник `media/base/<старый id>.png` ложится как
`media\base\<новый id>.png`; документ — только JSON-объект (копия — это ввод). Вид записи `screenshot`
переносится как есть (`bulk.rs::normalize`: ветка картинок для `k.is_image()`); совпала с имеющейся
`image` — остаётся `image`. Тест `screenshots_keep_their_documents_through_a_backup`.

### 17.6. Команды, ошибки, конкуренция

- `import_lt_probe` → `ImportProbe`; `import_lt_run` → `ImportReport` (`not_found:` — базы нет);
  `backup_export` → путь или `null`; `backup_import(mode)` → `ImportReport` (`invalid: cancelled` — диалог
  закрыт). Коды: `invalid:` — не та, битая или более новая база/копия; `failed:` — ввод-вывод; `busy:` —
  уже идёт другая операция импорта или копии (одна за раз; пробник не блокируется).
- Всё тяжёлое — на blocking-пуле; диалоги — асинхронно (`oneshot`), как `pick_exe`.
- Подписи диалогов (EN/RU) временно в `import/commands.rs` — до ключей в `i18n_native.rs` (сниппет
  координатору в отчёте W2-B).

### 17.7. `store/bulk.rs` (API для модуля `import`)

```rust
pub(crate) struct ImportItem { clip: NewClip /*copied_at = исходное*/, created_at, title, copy_count,
    used_at, use_count, favorite, fav_order: Option<f64>, folder: Option<String>, ocr_text, ocr_state,
    app_icon: Option<Vec<u8>> }
pub(crate) struct ImportSession { /* записи этого прогона, папки имя → id */ }
impl Store {
    fn retention_cutoff(&self, Retention) -> Option<i64>;      // now − срок; None = всегда
    fn fav_order_base(&self) -> Result<f64, String>;
    fn data_dir(&self) -> &Path;
    fn import_folders(&self, names, &mut ImportSession) -> Result<i64 /*создано*/, String>;
    fn import_batch(&self, Vec<ImportItem>, &mut ImportSession) -> Result<BatchStats, String>;
    fn merge_snippets(&self, &[SnippetRow]) -> Result<i64, String>;
    fn snapshot_to(&self, dest: &Path) -> Result<(), String>;
    fn replace_with(&self, staging: &Path, expected_clips: i64) -> Result<PathBuf /*aside*/, String>;
}
```

Нормализация `ImportItem` повторяет `write.rs::prepare` захвата (хэш, заголовок PNG и 64 МП, текст ≤ 2 МБ
и списки ≤ 2000 с перехэшированием, форматы ≤ 2 МБ, метрики, превью); `upsert_format`/`recompute_size`
продублированы для savepoint (`&Connection`). Хранилище по-прежнему не эмитит — эмитит `import`.

### 17.8. Ограничения

- Нативно не проверено (PQ-02): диалоги, переименование каталогов при открытых файлах, импорт с живой
  базой Liquid Tools — в чек-листе живой проверки.
- Двойники `apps`, заведённые до FIX-02 (импорт `<stem>.exe`, затем захват с полным путём), не
  сливаются задним числом: исправление предотвращает новые, существующие остаются двумя строками.
- `<stem>.exe` из Liquid Tools неоднозначен: если два разных exe с одним именем файла лежат в разных
  папках, строку импорта заберёт тот, кого захват увидит первым.
- Экспорт и `replace` временно занимают место: снимок базы во `%TEMP%` и `.part`; `replace` держит
  прежние данные в `backup-before-restore-<ms>\` до следующего восстановления, «Очистить всё» или
  «Удалить недавнее» (если копия сделана в окне) — W3, §4.2. Снимок экспорта во `%TEMP%` при падении
  процесса посреди экспорта остаётся (guard не срабатывает) — уборки при старте нет.
- `merge` переносит записи, папки и сниппеты, но не настройки; `replace` — всё, кроме геометрии окна
  настроек.
- Запись, сделанная в Liquid Tools во время копирования файлов, может не попасть в импорт (повторный
  импорт её добавит).

## 18. Скриншоты (`screenshot/`, `src/lib/editor/`, окна `capture` · `shots` · `editor` · `pin-*`) — W5–W8

> Замер: 2026-11-26 (W5-A: фундамент — контракт §18.10 в `model.rs` ≡ `types.ts` и §9.10, настройки,
> модуль `screenshot/` с командами-делегатами и заглушками, окно `capture` в `setup()`, маршруты `liq`,
> права, точки входа четырёх окон с заглушками, мок `mock/shots.ts`; гейты зелёные; нативно не
> запускалось; захвата, окон по требованию, редактора ещё нет — это W6). W6-A (2026-11-27): нативный
> захват, сессии, коммит, файлы, стор (схема v2), 9 хоткеев, трей, `--capture` — §18.3–18.5, §18.11
> «Как сделано»; `cargo test` 394/394 дважды. W6-B (2026-11-27): окна по требованию `shots` / `editor` /
> `pin-*` (§18.2, правило окон сверено с docs.rs tauri 2.11.2), нативная сторона QAO, редактора и
> закрепления (§18.8–18.9), статус `PrtSc`/«Ножниц» (§18.11), импорт снимков LT (§17.1). Нативно не
> запускалось (сценарии — в отчётах W6). W6-E (2026-11-27): фронт QAO и закреплённых окон — §18.8.1–18.8.2. Продукт — SPEC §9. Источник переноса и разбор его
> дефектов — `docs/prompts/_RESEARCH-W5-SCREENSHOT-SOURCE.md`; эталоны взаимодействия —
> `docs/prompts/_RESEARCH-W5-ANNOTATION-UX.md` (локальные файлы координатора, абсолютные пути — в
> промптах). Studio не переносится; всё, что в LT существует только ради studio (кнопка Mockup,
> флаг `studio`, заголовки `x-format`/`x-keep-open`, сайдкары `*.scene.json`, `html-to-image`), не
> переносится тоже (разведка §5.3).

### 18.1. Модули и владельцы (W6)

| Зона | Файлы | Агент W6 |
|---|---|---|
| Нативный захват и коммит | `src-tauri/src/screenshot/{grab,targets,session,frame,commit,files}.rs` · `store/shots.rs` + миграция схемы v2 + поддержка `screenshot` в `store/{feed,lifecycle,maint,media,bulk}.rs` · `hotkeys/*` (5 новых слотов) · `tray.rs` · `i18n_native.rs` | **W6-A** |
| Нативные окна и сервисы | `src-tauri/src/screenshot/{windows,qao,editor,pin,status}.rs` · `import/lt.rs` (снимки LT) | **W6-B** |
| Редактор аннотаций + окно редактора | `src/lib/editor/**` · `src/windows/editor/**` · `editor.html` · `tests/editor*.test.mjs` | **W6-C** |
| Оверлей захвата | `src/windows/capture/**` · `capture.html` · `tests/capture*.test.mjs` | **W6-D** |
| Миниатюры (QAO) и закреплённые окна | `src/windows/shots/**` · `src/windows/pin/**` · `shots.html` · `pin.html` · `tests/shots*.test.mjs` | **W6-E** |
| Шелф и настройки | карточка/пилюля/действия/токен в `src/windows/picker/**` · раздел `src/windows/settings/sections/Screenshot.svelte` + группа хоткеев + онбординг | **W6-F** |
| Узлы (W5-A, затем координатор) | `lib.rs` · `tauri.conf.json` · `capabilities/*` · `model.rs` + `src/lib/ipc/types.ts` · `ipc/{commands,events,runtime,frame}.ts` · `src/lib/ipc/mock/**` · `settings.rs` · `events.rs` · `screenshot/mod.rs` (команды-делегаты, `ScreenshotState`, маршруты) · `screenshot/body.rs` (кадрирование тела) · `vite.config.ts` · `*.html` · `i18n/{en,ru}.ts` (каждый агент дописывает свой блок-префикс) · `app.css` | **W5-A** |

Префиксы i18n: `editor.*` (C) · `capture.*` (D) · `shots.*`, `pin.*` (E) · `settings.shot.*`,
`picker.shot.*`, `onboarding.shot.*` (F). Нативные строки (трей, диалоги) — `i18n_native.rs` (A);
W6-B присылает свои строки сниппетом в отчёте.

### 18.2. Окна

| Окно | Создание | Свойства |
|---|---|---|
| `capture` | **заранее в `setup()`**, скрытое (захват должен показываться мгновенно) | прозрачное, без рамки, always-on-top, skip-taskbar, focusable, `shadow(false)`, `disable_drag_drop_handler`; размер/позиция = монитор захвата (физ. px, позиция → размер → позиция, как §7.4); `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`; `CloseRequested` → отмена сессии + hide |
| `shots` (QAO) | **по требованию** при первом снимке, затем живёт скрытым | прозрачное, без рамки, always-on-top, skip-taskbar, `focusable(false)` (`WS_EX_NOACTIVATE`; клики и drag-out работают без активации), показ через `show_no_activate`, `disable_drag_drop_handler`, `WDA_EXCLUDEFROMCAPTURE`; угол рабочей области монитора снимка; высота от числа карточек (`shots_hud_layout`) |
| `editor` | **по требованию**, одно; закрытие = уничтожение (память) | обычное окно: системный заголовок, на панели задач, мин. 640×420, по умолчанию вписать картинку в 80 % рабочей области; заголовок из `i18n_native` («Имя — Liqlip») |
| `pin-<n>` | **по требованию**, до 6; закрытие = уничтожение | прозрачное, без рамки, always-on-top, skip-taskbar, focusable, `shadow(false)`; размер = картинка в логических px (≤ 80 % монитора) |

**Исключение из правила «окна только в `setup()`»** (CLAUDE.md, правило 2). Документация Tauri
(`WebviewWindowBuilder::new`, раздел Known issues) говорит, что построение окна дедлочит на Windows
**в синхронной команде или обработчике событий**, и советует строить окна из `async`-команд и
отдельных потоков. W6-B **сверяет текст на docs.rs для нашей версии `tauri`** и строит
`shots`/`editor`/`pin-*` только так: из `async`-команды или `std::thread::spawn`, никогда из главного
потока, обработчика событий окна, колбэка хоткея или трея. Если сверка не подтвердит — запасной план:
`shots` и `editor` заранее в `setup()`, `pin` — одно окно (решение фиксируется в отчёте и ROADMAP §11).
Память в простое: до первого снимка добавляется только `capture`.

**Сверка (W6-B, 2026-11-27).** `Cargo.lock`: `tauri` 2.11.2. docs.rs
(`https://docs.rs/tauri/2.11.2/tauri/webview/struct.WebviewWindowBuilder.html`, тот же текст — в исходнике
крейта `webview/webview_window.rs`), `WebviewWindowBuilder::new`, раздел Known issues: «On Windows, this
function deadlocks when used in a synchronous command and event handlers, see [the Webview2 issue]. You
should use `async` commands and separate threads when creating windows.» (у `from_config` — то же, «…in a
synchronous command or event handlers»; у `build` своего раздела нет). Примеры там же: окно в `setup`, в
`std::thread::spawn`, в `async`-команде. **Решение: окна по требованию**, запасной план не нужен.

**Как сделано (`screenshot/windows.rs`).**
- `windows::init` (шаг 5 `setup()`, главный поток) запоминает id главного потока; `may_build()` отказывает в
  построении на нём (`failed: …`) — вместо зависания. `editor::open` / `pin::open`, вызванные с главного
  потока (трей, обработчик), уходят на короткий поток; QAO строится всегда на своём потоке. Ни один наш замок
  не держится, пока окно строится (построение ждёт главный поток).
- Новое окно получает нативную тему из `settings.general.theme` (`.theme(…)`, то же отображение, что
  `theme::apply`); тема и язык содержимого — как у остальных окон (`initAppearance()` на странице).
- Геометрия — физ. px, позиция → размер → позиция (`windows::place`); монитор — по имени устройства GDI
  (`ShotAdded.monitor`), иначе под курсором (в щели — ближайший), иначе первый (`pick_monitor`, тест).
- `shots` и `pin-*` показываются **без активации** Win32 `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE |
  SWP_SHOWWINDOW)` и прячутся `ShowWindow(SW_HIDE)` (как HUD §7.7: tao `show()` = `SW_SHOW` активирует);
  `WS_EX_TOOLWINDOW` (вне Alt+Tab) — `apply_toolwindow`, копия паттерна `picker/win.rs` (тот модуль
  приватный у шелфа). Показ и перестановка идут на главном потоке (`run_on_main_thread`): перемещения tao
  ставятся в его очередь, и Win32-показ не обгоняет их. Оба окна строятся с `focused(false)` (ни окно, ни
  WebView2 не берут фокус при создании).
- `editor` и `pin-*` уничтожаются при закрытии (`destroy()`); `WindowEvent::Destroyed` чистит состояние зоны.
  `shots` не уничтожается: `CloseRequested` → `prevent_close` + hide; если окно всё же умерло (сбой
  WebView2) — следующий снимок построит его заново.

`setup()` (§3): шаг 6 добавляет `capture`; шаг 7 применяет 5 новых хоткеев; новых `manage()` —
`ScreenshotState` (сессия, кадры, последняя область, стопка-счётчик окон) **до** создания окон.

### 18.3. Захват (W6-A)

- **Заморозка всех мониторов** при триггере, до показа окон: для каждого монитора
  `GetDC(None)` + `BitBlt(SRCCOPY | CAPTUREBLT)` своего прямоугольника → `GetDIBits` 32 bpp top-down
  (перенос `capture.rs` LT, разведка §4). Перед BitBlt: если открыт шелф — `picker` прячется и
  `DwmFlush()`; QAO прячется (у него ещё и `WDA_EXCLUDEFROMCAPTURE`). Порядок мониторов и геометрия —
  физические px виртуального экрана (Per-Monitor-V2).
- **Один буфер на монитор**: BMP целиком (`BITMAPFILEHEADER` + `BITMAPINFOHEADER` с отрицательной
  высотой + BGRA). Его отдаёт схема, из него же режется кроп для коммита. Никаких параллельных RGBA-копий
  (дефект П-21 LT).
- **Дерево окон** (`targets.rs` — перенос `CAP.rs:54-104, 239-354` с лимитами 2048 узлов / 32 уровня /
  35 мс на окно / 120 мс бюджет детей) для монитора курсора — до показа оверлея; для другого монитора —
  при переходе на него (событие `shot:session`).
- **Схема `liq`** (замыкание в `lib.rs` — W5-A вызывает `screenshot::serve_uri(path)` для префиксов
  `shot/` и `base/`): `/shot/<sessionId>/<monitorIndex>.bmp` → `image/bmp`; `/base/<clipId>?v=<n>` →
  исходник для редактора (PNG из `media/base/<id>.png`, если есть, иначе `media/<id>.png`). Оба —
  `Cache-Control: no-store`, ACAO `*` (canvas не должен быть tainted: `<img crossorigin="anonymous">`).
- **Следование за курсором**: пока сессия в этапе 1 (оверлей сообщает `shot_stage(sessionId, 'select'|'frame')`),
  поток сессии раз в 50 мс проверяет монитор курсора; сменился — окно переставляется на новый монитор
  (позиция → размер → позиция), строится дерево окон, `emit_to("capture", "shot:session", …)`.
- **Фокус**: в момент триггера — `paste::focus::capture_target()`; после закрытия оверлея (готово или
  отмена) — возврат фокуса этому окну (`bring_to_front`), чтобы `Ctrl+V` сразу вставлял. Захват без
  оверлея (экран, окно, повтор) фокус не трогает.
- **Режимы без оверлея**: `screen` — монитор курсора целиком; `window` — `GetForegroundWindow` →
  DWM-границы → кроп кадра его монитора; `repeat` — `last_region {monitorId, rect}` (только в
  памяти процесса); всё сразу в коммит.
- **Задержка** (трей): поток спит по секунде, каждую секунду `hud:show {kind:'countdown', text:'3'}`,
  затем обычный триггер.
- Повторный триггер при активной сессии — игнор (лог `debug`).

**Как сделано (W6-A, 2026-11-27).**

- **`grab.rs`** — мониторы `EnumDisplayMonitors` + `GetMonitorInfoW(MONITORINFOEXW)` (`id` =
  `szDevice`, тот же, что `Monitor::name` у Tauri/tao) + `GetDpiForMonitor(MDT_EFFECTIVE_DPI)` / 96;
  порядок слева направо, сверху вниз (`index` сессии). Win32 напрямую, без кругового рейса Tauri на
  главный поток. Заморозка — перенос LT: `GetDC(NULL)` → `CreateCompatibleDC/Bitmap` →
  `BitBlt(SRCCOPY | CAPTUREBLT)` → битмап снят с DC → `GetDIBits` 32 bpp top-down **прямо в буфер BMP
  кадра**; неполный `GetDIBits` — ошибка; все GDI-объекты освобождаются на любом пути. `monitor_at` —
  монитор точки, между мониторами — ближайший (как `MONITOR_DEFAULTTONEAREST`), тест.
- **`frame.rs`** — `Frame` = один `Vec<u8>`: `BITMAPFILEHEADER` + `BITMAPINFOHEADER` (32 bpp `BI_RGB`,
  отрицательная высота) + строки B,G,R,X, X → 255 (декодер, читающий 4-й байт как альфу, видит
  непрозрачную картинку). Схема отдаёт его как есть, коммит режет из него же (`crop_rgba`: B↔R, альфа
  255, один проход). Кадр держит сессия в `Arc`; `serve` клонирует `Arc` под замком и копирует байты
  уже без него (тело ответа схемы — `Vec`, одна временная копия на запрос). `clamp_crop` — перенос LT
  с его тестами; `rect_to_crop` — каждая грань `Rect` округляется до ближайшего пикселя, затем
  `clamp_crop` (**оверлей с аннотациями экспортирует ровно этот прямоугольник**). Маршруты
  `/shot/<id>/<index>.bmp` (только открытая сессия) и `/base/<id>` (`Store::serve_base`).
- **`targets.rs`** — перенос `enumerate_windows` + `child_regions` LT с лимитами (2048 узлов и 35 мс на
  окно, 32 уровня, 120 мс на всех детей; сначала окно под курсором, потом Chromium). Свои окна
  исключаются **по процессу** (`GetWindowThreadProcessId == GetCurrentProcessId`): оверлей, шелф, HUD,
  QAO, редактор, `pin-*`, настройки, диалоги — без списка меток и без обращений к Tauri. Чистые части
  (`local_region`, `tree`, `children`, `scan_order`, `keep_collecting`) — тесты.
- **`session.rs`** — `capture(mode, delay)`: `delay ≤ 10` (иначе `invalid`), занятость
  (`SessionState::begin` — токен подготовки; триггер при подготовке или открытой сессии игнорируется
  с логом), поток `liqlip-shot`: отсчёт `hud:show countdown` по секунде → `focus_context` → шелф
  прячется (`picker::hide_now`, если виден), HUD-тост прячется (`ShowWindow(SW_HIDE)` на главном
  потоке — последняя секунда отсчёта не попадёт в кадр), QAO прячется (`qao_bridge::hide_for_capture`),
  `DwmFlush` (если что-то спрятали) → мониторы → `screen` / `window` / `repeat` замораживают один
  монитор, снимают занятость сразу после заморозки и идут в `commit::direct`; `region` / `text` —
  все мониторы, дерево окон монитора курсора, `SessionState::open` (id сессий растут с 1),
  `shot:session`, затем окно: позиция → размер → позиция, `WS_EX_TOOLWINDOW` до и после `show()`,
  повтор геометрии, `set_focus` — всё на главном потоке с ожиданием ≤ 1,5 с (замок сессии в это время
  не держится). Поток следования (50 мс) работает, пока сессия открыта, и двигает окно только в этапе
  `select`. Ошибка заморозки → HUD `shot_failed` + лог + QAO назад.
- **Фокус и источник** (`focus_context`): окно переднего плана до любых наших окон; шелф впереди (или
  переднего окна нет даже через 200 мс — фокус в пути, например шелф закрылся из своего меню прямо
  перед командой) → сохранённая цель вставки шелфа `paste::focus::current()`; источник записи —
  `paste::focus::target_for` (только окно приложения). В отличие от `capture_target` цель вставки шелфа
  не меняется. Возврат фокуса — `paste::focus::bring_to_front(hwnd, 500 мс)`.
- **Режимы без оверлея**: `window` — границы DWM окна переднего плана (`DWMWA_EXTENDED_FRAME_BOUNDS`,
  иначе `GetWindowRect`), монитор — по центру окна, кроп обрезан краем монитора (§9.10); нет окна
  (рабочий стол) → весь монитор курсора. `repeat` — `last_region` (монитор по `id`); нет её или монитор
  пропал → обычный выбор области. `last_region` запоминает коммит оверлея (кроме `text`).
- **Конец сессии**: коммит забирает сессию (`session::take`) и закрывает её (`session::close`: скрыть
  окно → фокус → `shot:end`); отмена (`shot_cancel`, Alt+F4 → `on_capture_close` с главного потока
  уходит на короткий поток) — то же + кадры освобождаются + `qao_bridge::restore_after_capture`.
  Устаревший `shot_stage` / `shot_cancel` молча игнорируются; `shot_commit` устаревшей сессии —
  `not_found: session`.
- **`liqlip --capture [region|screen|window|repeat|text]`** (и `--capture=<mode>`): разбор
  `session::capture_arg` (тест), запуск `session::handle_args` — подключает координатор в колбэке
  single-instance (сниппет в отчёте W6-A).
- **Шелф и оверлей**: триггер прячет шелф до показа оверлея, так что одновременно они не открыты;
  если шелф вызвать поверх открытого оверлея, `picker::Hwnds` знает окно `capture` (W6-A): клик по
  оверлею — «клик снаружи», а не «свой диалог», и оверлей не становится окном возврата фокуса шелфа.
- **Хуки W6-B** (`session::qao_bridge`, временные заглушки до интеграции): `hide_for_capture`,
  `restore_after_capture`, `show_added(payload)` → `qao::*` B; пока `show_added` только шлёт
  `shot:added`.

### 18.4. Коммит (W6-A)

`shot_commit` принимает **сырое тело** (Tauri raw request body, как `screenshot_commit_raw` LT) в
кадрировании, не зависящем от лимитов заголовков:

```
[u32 LE: N] [N байт UTF-8 JSON: ShotCommit] [RGBA: width*height*4 байт или 0 байт]
```

- `rect` — физические px в системе кадра монитора сессии; Rust зажимает в кадр (`clamp_crop` LT + тесты).
- Нет аннотаций (`doc == null` или `objects` пуст) → RGBA не передаётся, Rust режет кадр сам (быстрый путь).
- Есть аннотации → webview прислал сведённый RGBA кропа (WYSIWYG — тот же рендер, что на экране);
  Rust дополнительно режет **исходник** того же `rect` из кадра → `media/base/<id>.png`; документ
  (координаты относительно кропа) → `shot_docs.doc`.
- `action`: `done` — действия из `settings.screenshot` (copy/save/history/afterCapture); `copy` —
  только буфер + история; `save` — папка + история; `save_as` — диалог (папка по умолчанию — настройка)
  + история; `pin` — история + закрепить; `text` — OCR кропа (`ocr::Engine::recognize_png`), текст в буфер
  (`paste::copy_text`), HUD `text_copied`/`text_empty`; запись не создаётся.
- Порядок, оптимизированный под «Ctrl+V сразу»: (1) скрыть оверлей и вернуть фокус; (2) PNG/JPG
  кодирование на воркере; (3) буфер **первым** (`paste::writer` — PNG + DIB одной транзакцией,
  подавление собственной записи; нужен `pub(crate)`-хелпер `paste::copy_png(&[u8])`); (4) запись в
  стор (`store::shots::insert_screenshot`, тип `screenshot`, источник — приложение с фокусом в момент
  триггера, миниатюра); (5) файл в папку (`files.rs`: шаблон имени, уникализация `(2)`, создание
  папки); (6) `emit shot:added` → QAO (или открыть редактор / ничего по `afterCapture`); (7) звук
  играет QAO. Ошибка любого шага после (3) не отменяет предыдущие; ошибка сохранения → `error:
  'save_failed'` в `shot:added`.
- Тумблер типа `screenshot` в `storage.types` выключен → шаг (4) пропускается, QAO получает `clip:
  null` и работает с временным файлом в `%TEMP%\Liqlip\shots\` (удаляется при закрытии карточки и на
  старте).
- Лимит 64 Мп (§2.3) — не в историю, но буфер и папка работают.
- Форматы: PNG (по умолчанию) и JPG (`image` с фичей `jpeg`, качество из настроек); в историю всегда
  PNG (как все картинки), формат касается файла в папке и буфера не касается (в буфер — PNG + DIB).

**Как сделано (W6-A, 2026-11-27).**

- `commit::commit` (команда `shot_commit`, `async` → воркер рантайма): `session::take` → **сразу**
  `session::close` (окно не остаётся на экране и при ошибке запроса) → `rect_to_crop` (вне кадра —
  `invalid`) → `finish`. Режим сессии `text` всегда даёт действие `text`. Кадры освобождаются после
  коммита, затем `qao_bridge::restore_after_capture`.
- Картинка (`picture`): без тела — кроп кадра (документ, если прислан объектом, хранится как есть);
  тело размера кропа — сведённый RGBA, кроп кадра → `base_png`, документ хранится; тело другого
  размера — берётся как картинка (это видел пользователь), но без исходника и документа (лог).
- План (`commit::plan`, чистая функция, тест): `done` — `copy`/`save`/`afterCapture` из настроек;
  `copy` — буфер; `save` — папка; `save_as` — диалог; `pin` — закрепить; история — всегда, если
  включён тип `screenshot` и ≤ 64 Мп (`keeps_history`, тест).
- Шаги: PNG (`PngEncoder`, по умолчанию быстрое сжатие) → буфер `paste::copy_png(png, Some((w, h,
  rgba)))` (DIB строится из имеющихся пикселей без декодирования PNG; запись помечена своей — capture её
  не записывает) → `Store::insert_screenshot` (хэш `content::image_hash`, миниатюра ≤ 360 px из RGBA,
  `unit` = масштаб монитора, источник) + `clips:changed {origin:'capture'}` + иконка приложения из кэша
  `capture::app_icon_data_url` (если её ещё нет) → файл (`files::save_new` / `save_new_as`) →
  `set_saved_path` → QAO / редактор (`editor::open`) / закрепление (`pin::open`) / ничего. Ошибка
  сохранения → `error: 'save_failed'`, и QAO показывается даже при `afterCapture: none`. Нет записи
  (тип выключен / > 64 Мп) → QAO получает `tempPath` (`%TEMP%\Liqlip\shots\shot-<ms>-<n>.png`; папка
  чистится на старте `session::init`, карточки — W6-B); `editor` без записи → QAO.
- `action: 'text'` → `ocr::recognize_png(app, png)` (новый `pub(crate)`: свой поток в MTA WinRT,
  обычный приоритет, язык `ocr.language`, без проверки `ocr.enabled` — явное действие над картинкой,
  которую никто не хранит; ждёт ≤ 10 с) → `paste::copy_text` + HUD `text_copied`; пусто → HUD
  `text_empty`; ошибка распознавания (нет языков) → ошибка команды (HUD-вида для неё в контракте нет).
- «Сохранить как…» коммита — `blocking_save_file` диалог-плагина (документирован для `async`-команд,
  никогда не главный поток), **без родителя** (оверлей уже скрыт, шелф скрыт).
- **Коммит редактора** (`shot_editor_commit`) оркестрирует W6-B (`editor::commit`, решение координатора);
  `commit::editor_commit` остаётся делегатом-заглушкой до интеграции. Шаги W6-A для B:
  `commit::apply_edit(app, id, rgba, w, h, doc) -> (Clip, png)` (PNG, хэш, миниатюра,
  `Store::apply_edit`, `clips:changed {origin:'user', change:'updated'}`), `commit::base_rgba(app, id)`,
  `paste::copy_png`, `files::overwrite_saved(app, id, png)`, `files::save_png_as(app, id, png).await`.

### 18.5. Хранение (W6-A)

- `Kind` += `screenshot` (аддитивно: `model.rs` + `types.ts`); все `match`/`switch` по `Kind` — Rust и
  TS — получают ветку (для вставки, копирования, drag-out, OCR, превью, Quick Look `screenshot` ведёт
  себя как `image`). `clips.kind` — `TEXT` без CHECK — миграция таблицы не нужна (W6-A сверяет).
- **Схема v2** (`PRAGMA user_version = 2`, миграция в `store/schema.rs`, идемпотентна, тест):
  ```sql
  CREATE TABLE shot_docs (
    clip_id    INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
    doc        TEXT NOT NULL,       -- AnnotDoc v1 (JSON, §18.6); '{"v":1,…,"objects":[]}' у снимка без аннотаций
    has_base   INTEGER NOT NULL,    -- 1 = media/base/<id>.png хранит исходник (есть аннотации или было редактирование)
    saved_path TEXT,                -- последний файл в папке снимков (перезапись при «Готово», «Показать в папке»)
    updated_at INTEGER NOT NULL
  );
  ```
  Строка есть у каждого `screenshot` и у любой `image`, которую хоть раз редактировали.
- Файлы: `media/base/<id>.png` (исходник). Удаление записи → удаление файла тем же путём, что
  `media/<id>.png` (`maint`/`media`), сироты `media/base/*` чистит фоновый поток.
- `store::shots`: `insert_screenshot(NewShot) -> InsertOutcome`, `doc(id) -> Option<ShotDoc>`,
  `apply_edit(id, flattened_png, base_png: Option, doc, w, h) -> Clip` (обновляет `media/<id>.png`,
  миниатюру, `hash`, `width/height`, `size`, `shot_docs`; для `image` при первом редактировании сохраняет
  исходник; событие `clips:changed {origin:'user', change:'updated'}`), `set_saved_path(id, path)`.
- Дедуп по хэшу: снимок, совпавший пиксель-в-пиксель с существующей записью, поднимает её наверх (как
  копирование). Если совпала запись `image` — тип не меняется.
- Лента и счётчики: фильтр пилюли «Картинки» = `image` + `screenshot`; новая пилюля «Скриншоты» =
  `screenshot`; `Counts` += `screenshot`; токен `type:screenshot` | `type:shot`. Ретенция/лимиты —
  как для всех.
- Резервная копия `.liqlip` (`store/bulk.rs`, §17): таблица `shot_docs` и `media/base/*` входят в
  экспорт, восстанавливаются в `replace` и `merge` (в `merge` — с новым id записи).
- Импорт LT (W6-B, `import/lt.rs`): записи LT `kind='screenshot'` (файл по `content_ref` в кэше LT)
  → `screenshot` без документа аннотаций; отсутствующий файл — `skipped`.

**Как сделано (W6-A, 2026-11-27).** Схема — §4.2a. `store/shots.rs`:

| API | Что делает |
|---|---|
| `insert_screenshot(NewShot) -> InsertOutcome` | `insert_clip` (вид `screenshot`), затем вторая короткая транзакция — строка `shot_docs` (пустой документ `{"v":1,"w","h","unit","crop":null,"objects":[]}`, если не прислан) + `media\base\<id>.png` + `meta.shot`. Строка не записалась — запись есть и открывается как простая картинка (лог). У поднятой записи со строкой документ не трогается (только новый `saved_path`) |
| `shot_doc(id) -> Option<ShotDoc>` | строка (`doc` как JSON, `has_base`, `saved_path`, `updated_at`); `not_found` для отсутствующей/удалённой записи (§18.5 `doc(id)`) |
| `apply_edit(id, ShotEdit) -> Clip` | новая картинка, миниатюра, хэш, размеры, `size`, документ, `has_base = 1`, `updated_at` растёт строго; первая правка `image` сохраняет текущую картинку как исходник; хэш, занятый другой записью, получает суффикс `-e<id>` (правка не сливает записи); OCR заново (`pending`, если OCR включён). Файлы ставятся в конце транзакции: исходник → миниатюра → картинка |
| `set_saved_path(id, Option<&str>)` | у `image` без строки создаёт её с пустым документом |
| `shot_source(id)`, `shot_clip(id)`, `serve_base(id)` | данные для файлов/редактора/пина; запись как в ленте; байты `/base` (исходник, иначе картинка) |
| `import_shot_docs(Vec<ShotDocImport>)` | merge бэкапа, §17.5 |

- **Дедуп** (`write.rs`): при совпадении хэша картинка сохраняет свой вид — снимок, равный картинке
  `image`, поднимает её (вид `image`, строки нет, пока её не сохранят/не отредактируют); копия снимка из
  другого приложения поднимает снимок.
- **Ветки по виду**: `write.rs` (`prepare`, OCR при вставке), `feed.rs` (`formats`, `thumbUrl`,
  `imageUrl`), `media.rs` (`/thumb`, сироты), `ocr.rs` (`kind IN ('image','screenshot')`; следующая
  задача — по одному поиску по индексу на вид, без сортировки), `bulk.rs`, `import/backup.rs`.
  `Counts.kinds.screenshot` даёт прежний `GROUP BY kind`; пилюли и токен `type:screenshot|shot`
  разбирает фронт (W6-F) — фильтр `kinds` в Rust уже принимает любой набор видов.

**UI (W6-F, как сделано; подробно — `docs/picker.md` §5.10, `docs/settings.md` §4–5).** Замер: 2026-11-27
(браузер на моке, обе темы, RU/EN; нативно не запускалось).
- Шелф: карточка `screenshot` = картинка + камера в мета-строке, бейдж `PNG`; пилюля «Скриншоты» после
  «Картинок», «Картинки» считают `kinds.image + kinds.screenshot`; токен `type:screenshot` (`shot`,
  `скриншот`, `снимок`…; `type:image` — только `image`); меню действий картинки и снимка —
  Редактировать (`Ctrl+E`) · Закрепить (`Ctrl+P`) · Сохранить как… · «Показать в папке» вместо «в
  Проводнике», когда есть `meta.shot.savedPath`; «Снимок экрана» в конце меню записей и как всё меню
  без записи (`picker_hide(true)` → `shot_capture('region')`); Quick Look — «Сохранён» / «Аннотации»,
  `E` и `Ctrl+E` → `shot_edit`, `Ctrl+P` → `shot_pin`. В сетке голая `E` остаётся печатью в поиск.
  Перед `shot_edit` / `shot_pin` шелф прячет себя после команды, перед `shot_save_as` — до неё (окна и
  диалог — нашего процесса, `picker::blur_guard` шелф не закрывает, а он поверх всех).
- Настройки: раздел `screenshot` (после «Сниппетов») по SPEC §9.7; предупреждение про «Ножницы» по
  `shot_status.snippingKey`, пока какой-то слот — голая `PrtSc`; группа хоткеев «Скриншоты» (ошибки и
  из `settings_set`, и из `shot_status.hotkeyErrors`); шаг онбординга «Скриншоты» перед «Готово».
  Папку пока не выбрать и не открыть: нет команд выбора папки и `open_path` для папки снимков —
  поле только чтения, кнопки выключены (запрос координатору в отчёте W6-F). Предпросмотр имени файла:
  `{date}` = `yyyy-MM-dd`, `{time}` = `HHmmss` (как «Ножницы» Windows; без `:`) — `files.rs` (W6-A) и
  мок должны давать то же (мок W5-A пишет `HH.mm.ss`).
- HUD: `countdown` — секунды крупной цифрой в ink-круге 32 px в конце капсулы (концентрично), тики
  одного отсчёта не переигрывают вход, только цифру.

### 18.6. Редактор аннотаций (`src/lib/editor/`, W6-C)

**Модули** (чистые `.ts` тестируются node:test, как остальные): `doc.ts` (типы + (де)сериализация +
миграции версий) · `geometry.ts` (перенос `annotationGeometry.ts` LT + тесты) · `textLayout.ts`
(перенос `annotationText.ts` LT + тесты) · `hit.ts` (попадания: контур/площадь, допуски в экранных px)
· `render.ts` (рисование документа на canvas: базовая картинка, слои §SPEC 9.3-9, выделение/ховер
отдельным оверлейным canvas) · `effects.ts` (плашка/пиксели/размытие от исходных пикселей) ·
`history.ts` (стек снимков массива объектов со структурным разделением; склейка серий < 500 мс) ·
`tools/*.ts` (машина состояний по инструментам) · `interaction.ts` (диспетчер указателя и клавиш по
правилам §SPEC 9.3) · `export.ts` (сведение в RGBA кропа в пикселях исходника) · `defaults.ts`
(умолчания инструментов в `localStorage` `liqlip.editor.tools`) · UI: `Editor.svelte` (холст),
`Toolbar.svelte`, `PropsBar.svelte`, `ColorPopover.svelte`, `TextInput.svelte`, `Cheatsheet.svelte`,
`glyphs/*` (свои SVG-глифы инструментов в формате `src/lib/heroicons/`: у Heroicons нет прямоугольника,
эллипса, маркера, стрелки-инструмента — разведка §5.2).

**Документ аннотаций v1** (`AnnotDoc`, хранится в `shot_docs.doc`; координаты — **пиксели исходника**):

```ts
export type Pt = [number, number];
export interface Rect { x: number; y: number; w: number; h: number }
export type Size = 1 | 2 | 3 | 4 | 5;
interface Obj { id: string; color: string /* #rrggbb */ }
export type AnnotObject =
  | Obj & { kind: 'arrow'; a: Pt; b: Pt; bend: number; style: 'straight' | 'curved' | 'double'; size: Size }
  | Obj & { kind: 'line'; a: Pt; b: Pt; size: Size }
  | Obj & { kind: 'rect' | 'ellipse'; r: Rect; fill: boolean; size: Size }
  | Obj & { kind: 'pen' | 'marker'; points: Pt[]; size: Size }
  | Obj & { kind: 'text'; at: Pt; width: number | null; text: string; fontSize: number; style: 'plain' | 'badge' }
  | Obj & { kind: 'counter'; at: Pt; n: number | null; size: Size }   // n=null — авто (предыдущий + 1)
  | Obj & { kind: 'redact'; r: Rect; mode: 'solid' | 'pixelate' | 'blur'; size: Size }
  | Omit<Obj, 'color'> & { kind: 'spotlight'; r: Rect; shape: 'rect' | 'ellipse' }
  | Omit<Obj, 'color'> & { kind: 'image'; r: Rect; src: string /* data:image/png;base64,… */ };
export interface AnnotDoc {
  v: 1;
  w: number; h: number;      // размер исходника
  unit: number;              // px исходника на 1 логический px интерфейса в момент создания (масштаб монитора; 1 — картинка из буфера)
  crop: Rect | null;         // неразрушающая обрезка (окно редактора)
  objects: AnnotObject[];    // порядок внутри слоя = z-порядок
}
```
- Толщина/кегль: `px = TABLE[size] * doc.unit` (`TABLE = [2, 3, 5, 8, 12]` для линий; шрифт — `fontSize *
  unit`) — снимок 4K на 200 % получает визуально те же толщины, что на экране.
- Неизвестная `v` при загрузке → открыть исходник без объектов + тост; лишние поля игнорируются.
- **Рендер**: холст хранит исходник один раз (`ImageBitmap`); документ перерисовывается целиком при
  изменении состояния (не в rAF-цикле); ховер, выделение, ручки, лупа — на отдельном лёгком canvas.
  Тексты кэшируют раскладку по `(text, width, fontSize)`.
- **Эффекты**: считаются по пикселям **исходника** в области + поле 2·радиус (края не просвечивают —
  дефект П-3 LT); плашка — заливка цветом объекта; пиксели — ячейка `max(6, 4 · TABLE[size] · unit)`;
  размытие — радиус `3 · TABLE[size] · unit`, затем обрезка по прямоугольнику. Кэш по `(r, mode, size)`.
- **Экспорт** (`exportRGBA(rect)`): canvas размера `rect` в px исходника, `drawImage` исходника, затем
  тот же `drawDoc` — WYSIWYG по построению; результат `getImageData` → `Uint8Array` для коммита.
- **Компонент**:
  ```ts
  // Editor.svelte
  props: {
    base: ImageBitmap;              // декодированный исходник (оверлей — кадр монитора; окно — исходник записи)
    doc: AnnotDoc;                  // начальный документ (новый снимок — пустой, unit = масштаб монитора)
    mode: 'overlay' | 'window';     // overlay: 1 px исходника = 1 физ. px, без зума; window: зум/панорама
    clip?: Rect | null;             // overlay: рамка (рисование и экспорт — внутри; меняется извне при ресайзе рамки)
    onchange?: (doc: AnnotDoc) => void;   // каждый шаг истории
    onintent?: (i: 'done' | 'copy' | 'save' | 'save_as' | 'pin' | 'text' | 'close') => void;  // Enter, Ctrl+C/S/P …
  }
  // bind:this → api
  interface EditorApi {
    handleKey(e: KeyboardEvent): boolean;    // true — событие съедено (хост проверяет первым делом)
    escape(): boolean;                       // шаг лестницы Esc; false — разматывать нечего (хост решает о закрытии)
    hasObjects(): boolean; dirty(): boolean;
    doc(): AnnotDoc;                         // текущий документ (координаты — px исходника)
    exportRGBA(rect: Rect): Promise<{ width: number; height: number; data: Uint8Array }>;
    setTool(t: ToolId): void; tool(): ToolId;
  }
  ```
  `Toolbar.svelte` и `PropsBar.svelte` — отдельные компоненты, которые хост размещает сам (оверлей —
  у рамки; окно — сверху) и связывает с тем же состоянием редактора (общий `$state`-стор
  `createEditorState()`).
- **Тесты-правила**: каждое правило SPEC §9.3 (1–12) — тест на чистом диспетчере `interaction.ts`
  (синтетические pointer/key события → документ/выделение/инструмент), плюс hit-тесты контура,
  перенумерация счётчиков, склейка истории, сериализация v1, эффект без просвечивания краёв
  (сравнение пикселей на маленьком холсте через `OffscreenCanvas`-заглушку или чистую функцию над
  массивом).

### 18.7. Оверлей захвата (`src/windows/capture/`, W6-D)

> Замер: 2026-11-27 (W6-D, ветка `w6/capture-ui`): этапы 1 и 2, коммит, лестница `Esc`, текстовый и
> «быстрый» режимы проверены в браузере на моке (`pnpm dev --port 1452`, `/capture.html`, флаги
> `?mock-scale=1.5|2`, `?mock-shot=text`, `?mock-shot-quick=1`, `?mock-ocr=nolang`; обе темы); редактор —
> **временная заглушка** с API §18.6 (W6-C делает настоящий параллельно); нативно не запускалось (захват,
> фокус, следование за курсором и DPI — INT-5 / Eddy).

**Модули.** Чистые (node:test): `captureTargets.ts` (перенос LT как есть: `targetAt(windows, x, y,
wholeWindow)`), `geometry.ts` (два пространства — px кадра и px окна; область, рамка, ручки, попадания),
`dock.ts` (панель, чип размеров, лупа), `loupe.ts` (математика лупы), `machine.ts` (машина состояний,
выбор сессии, заголовок коммита, лестница `Esc`), `frameDoc.ts` (документ → координаты рамки,
консервативные границы объектов). UI: `Capture.svelte` (корень-хост: сессия, указатель, клавиши,
коммит), `Shade.svelte` (затемнение с дырой + кольцо + зоны «снаружи»), `Loupe.svelte`,
`FrameChrome.svelte` (ручки, чип), `Panel.svelte`, `HintBar.svelte`, `Notice.svelte`, `glyphs/Pin.svelte`
(своя булавка в формате Heroicons — у Heroicons её нет). Заглушка редактора — `editorStub.ts` +
`stub/*.svelte` (см. ниже «Опора на API редактора»).

**Сессия.** На монтировании — подписки `shot:session` и `shot:end`, затем `shot_session()`; оба пути
идемпотентны (`sessionChange`): другой `id` — новая сессия (сброс всего, `createEditorState()`,
`ocr_status` для кнопки «Текст», `shot_stage(id, 'select')`); тот же `id` на этапе 1 — сессия переехала
на другой монитор / получила дерево окон (берутся новые данные, кадр перезагружается, если сменился
`frameUrl`); тот же `id` позже — игнор. `shot:end` своей сессии или успешный коммит/отмена → `idle`:
`<img>` снят, `ImageBitmap` закрыт, редактор размонтирован — в простое оверлей ничего не держит.
Кадр — `<img crossorigin="anonymous" src=frameUrl>` (атрибут `crossorigin` статический — стоит до
`src`) → `createImageBitmap` → лупа и `Editor`; до загрузки работают ховер окон и клики (Rust режет кадр
сам); ошибка загрузки → тост, лупа и рисование недоступны, выбор и коммит работают.

**Координаты.** Указатель → px кадра: `k = frame.width / innerWidth` (по вертикали — `height /
innerHeight`; в окне они равны), позиция курсора — целая точка сетки `0…width` (`framePoint`): правый и
нижний край достижимы. Хром (ручки, панель, лупа) раскладывается в px окна. Проверено при `scale` 1 / 1.5 /
2 (тест + браузер: область `(160,160)→(480,320)` px окна при кадре 2880×1620 и окне 1280 → `rect
{360,360,720,360}`).

**Этап 1 (`select`).** Затемнение 20 % — один SVG-путь с even-odd дырой (без швов на дробных
координатах), дыра = окно под курсором (`targetAt`; вложенные поверхности; `Alt` — внешнее окно; рабочий
стол — весь монитор), кольцо выбора внутри дыры (белое + тёмная кромка — маркер поверх снимка, не элемент
интерфейса). Перерисовка — только при смене цели (никакого rAF-цикла, дефект П-20). Нажатие → `press`;
сдвиг ≥ 5 px окна по любой оси → область (`regionFrom`: `Shift` — квадрат, `Alt` — от центра, оба —
квадрат от центра; квадрат у края остаётся квадратом; `Space` держать — область едет за курсором);
отпускание без сдвига — окно под курсором (или монитор). Чип `W × H` над областью — только если
`session.dimensions` (дефект П-14); подпись лупы — размер при драге (если размеры включены), иначе
координаты пикселя. Лупа (алгоритм LT `C.s:1560-1635` / ShareX: круг, ближайший сосед, сетка, перекрестие
в клетку, центральный пиксель; хром белый/чёрный): по умолчанию `session.magnifier`, `M` — вкл/выкл,
колесо — увеличение (клетка 4…30 px окна, по умолчанию 10 = 15 пикселей; размер линзы ~150 px
постоянный), у правого/нижнего края переворачивается. Стрелки — виртуальный курсор на 1 px кадра
(`Shift` — 10; мышь возвращает реальный), `Enter` — начать область в курсоре, второй `Enter` — взять её
(пустую — цель под курсором); `F` — весь монитор, `R` — `session.lastRegion` (подсказка `R` — только если
она есть), `Esc` — отменить драг/клавиатурную область, иначе `shot_cancel`. Внизу по центру — строка
подсказок с клавишами (уезжает наверх, когда курсор рядом; прячется при драге; указатель не берёт).

**Этап 2 (`frame`).** Затемнение 30 % вокруг рамки, кольцо рамки **снаружи** (крайние пиксели снимка
видны). `Editor` (`mode:'overlay'`, `base` — кадр всего монитора, `doc` — пустой документ размера монитора,
`unit = monitor.scale`, `clip` = рамка, меняется вживую при движении/ресайзе) лежит под затемнением;
вокруг рамки — четыре прозрачные зоны «снаружи» (`Shade`), которые берут указатель, а дыра пропускает
события к редактору — **внутри рамки всё получает редактор**. `hitFrame`: ручки (8, центры в 12 px
снаружи края, радиус попадания 8; боковые скрыты на стороне < 40 px; у края монитора ручка втягивается
внутрь и по-прежнему главнее) → внутри → полоса 10 px снаружи (двигает) → снаружи (драг — новая область,
**только при инструменте «Выделение»**; клик снаружи — ничего). Чип размеров над левым верхним углом
(двигает рамку; нет места или над ним панель — внутри угла, в 20 px от угловой ручки; скрыт при
`dimensions=false`). `Ctrl+стрелки` — двигать рамку на 1/10 px, `Ctrl+Alt+стрелки` — размер от
правого/нижнего края. Жесты хоста захватывают указатель на корне (переживают смену зон под ним);
отпускание за окном ловится по `buttons === 0`.

**Панель** (`Panel.svelte`): поповер r24/pad 8; `Toolbar` · `PropsBar` редактора (хост передаёт их
сниппетами — редактор импортируется в одном месте) · Текст · Закрепить · Сохранить как… · Копировать ·
Отмена (иконки-кнопки 32 px, подсказки с клавишами `Ctrl P` / `Ctrl Shift S` / `Ctrl C` / `Esc`) ·
**Готово** (основная, `Enter`). Кнопки не забирают фокус (mousedown гасится) — `Enter`/`Space` после клика
идут в клавиши оверлея (П-25); фокус с клавиатуры работает (`Enter` на сфокусированной кнопке нажимает её).
OCR недоступен (`ocr_status().available = false`) → «Текст» `aria-disabled` с подсказкой «нужен языковой
пакет» (не `disabled` — иначе подсказка не видна). Стыковка `dockPanel`: снизу снаружи (зазор 24 px — за
ручками) → сверху снаружи → внутри у нижнего края (16 px); по центру рамки, прижата к краям монитора
(отступ 8). Во время любого жеста (хоста или редактора — `pointerdown` в фазе захвата на обёртке
редактора) панель `pointer-events: none`, а под указателем — `opacity .25`.

**Клавиши** (`KeyboardEvent.code`): этап 2 — сначала `editor.handleKey(e)` (вернул `true` → всё); затем
`Esc` → лестница; в поле ввода редактора хост больше ничего не трогает; `Enter` → `done`, `Ctrl+C` →
`copy`, `Ctrl+S` → `save`, `Ctrl+Shift+S` → `save_as`, `Ctrl+P` → `pin`, `Ctrl(+Alt)+стрелки` — рамка.
Двойной клик в «Выделении» на пустом месте (`isEmptyAt` по консервативным границам объектов, допуск 6 px
окна) → `done`; `onintent` редактора (`done/copy/save/save_as/pin/text`) → тот же коммит, `close` → последний
шаг лестницы. Повторный запрос во время `committing` игнорируется. Браузерные клавиши WebView2 глушатся
(`isBrowserShortcut`), контекстное меню — тоже (правый клик во время жеста отменяет жест хоста/объект
редактора, иначе ничего).

**Лестница `Esc`** (`escapeLadder`): жест хоста (драг области, движение/ресайз рамки — рамка
возвращается) → `editor.escape()` (жест, текст, выделение, «Выделение») → нет объектов: `shot_cancel`;
есть объекты: плашка «Закрыть без сохранения? `Esc` — да» на 3 с, второй `Esc` в это время — отмена
(по истечении — снова плашка). Кнопка «Отмена» и `onintent('close')` — только последний шаг (с объектами —
та же плашка). Ошибка `shot_cancel` → тост, оверлей остаётся (прозрачное окно поверх всего без интерфейса
было бы хуже).

**Коммит** (`commit`): `startCommit` → `committing` (кнопки неактивны, «Готово» с индикатором, курсор
ожидания, клавиши глушатся) → рамка округляется и зажимается (`snapRect`) → если есть объекты и после
`docForFrame` (сдвиг на `−рамка.x/y`, `w/h` = рамка, `crop: null`, объекты целиком вне рамки отброшены)
хоть один остался — `editor.exportRGBA(рамка)` + документ в заголовке; иначе тело без пикселей и `doc:
null` (Rust режет кадр сам) → `shotCommit(header, rgba)` → `idle`. `text` — всегда без пикселей (OCR читает
экран, а не аннотации). Ошибка → `commitFailed` (назад на этап, рамка и рисунок целы) + тост «Не удалось
сделать снимок. <причина по коду>».

**Режимы.** `text` (OCR): только этап 1 — любая цель (окно, область, `F`, `R`, `Enter…Enter`) сразу
`shot_commit({action:'text'})`; панели нет. `session.quick`: этап 2 пропускается — цель сразу `done`.

**Где код уточняет §18.7 / SPEC §9.2.**
- `F` и `R` — такие же цели, как окно и область: этап 2 (или сразу коммит при `quick`/`text`), а не
  мгновенный снимок экрана. Мгновенный снимок экрана — хоткей `Ctrl+PrtSc` (W6-A).
- Новая область драгом снаружи — только при инструменте «Выделение» (SPEC: «без активного
  инструмента»); с инструментом рисования нажатие снаружи ничего не делает.
- Затемнение этапа 2 — 30 % (этапа 1 — 20 %, как в SPEC): рамка заметнее на светлом содержимом.
- Двойной клик → «Готово» хост определяет по консервативным границам объектов (стрелка — по концам
  + 4 толщины + `|bend|` как смещение в px; текст — оценка ширины по символам). Точнее может редактор —
  тогда он шлёт `onintent('done')` сам; второй запрос безвреден.
- Кольца выбора и ручки — белое + тёмная кромка поверх снимка (единственная «обводка» оверлея: маркер
  на пользовательском содержимом, а не рамка интерфейса); цвет в лупе — только снимок.

**Опора на API редактора (§18.6) — сверить с W6-C при интеграции.** Оверлей импортирует редактор в
**одном месте** (`Capture.svelte`: `./editorStub` → `$lib/editor`). Что используется:
1. `createEditorState()` — один на сессию; передаётся пропом **`state`** в `Editor`, `Toolbar`,
   `PropsBar` (в списке пропов `Editor` в §18.6 его нет — имя `state` выбрано оверлеем).
2. `<Editor bind:this state base doc mode="overlay" clip onintent>`; `onchange` не используется.
   `clip` меняется извне при каждом движении/ресайзе рамки; события редактор получает только внутри
   рамки (снаружи их перехватывает хост), но его холст — на весь монитор.
3. `EditorApi`: `handleKey(e)` (первым на каждое нажатие этапа 2), `escape()`, `hasObjects()`, `doc()`
   (координаты — px всего кадра монитора), `exportRGBA(rect)` (rect — px кадра; `width/height` результата
   = rect), `tool()` (сравнивается только с `'select'`). `dirty()` и `setTool()` не используются.
4. `ToolId` содержит `'select'`; `EditorIntent` = `done | copy | save | save_as | pin | text | close`.
5. Правый клик во время своего жеста редактор обрабатывает сам; на шаге «Выделение» `escape()`
   возвращает `true`, пока переключает инструмент.

**Тесты** (node:test, 34): `captureTargets` (5 тестов LT + `WinRect` мока), `captureGeometry` (перевод при
`scale` 1/1.5/2, клик/драг, `Shift`/`Alt`/квадрат у края, `snapRect`, движение/ресайз/переворот рамки,
`Ctrl(+Alt)+стрелки`, ручки снаружи и втягивание у края, `hitFrame`, скрытие боковых ручек, лупа),
`captureDock` (снизу / сверху / внутри для полного экрана и высокой рамки / прижим слева и справа / панель
шире монитора, чип, лупа у краёв), `captureMachine` (машина состояний, `text`/`quick`, ошибка коммита,
чужой `shot:end`, `sessionChange`, заголовок §18.4, лестница `Esc`, `docForFrame`, границы, `isEmptyAt`).

### 18.8. QAO и закрепление (`src/windows/shots/`, `src/windows/pin/`, W6-E; нативное — W6-B)

- QAO держит стопку на фронте (`stack.ts` LT как основа: до 4, новая снизу); на каждое `shot:added`
  — карточка + звук (WebAudio, синтез щелчка ~60 мс, если `settings.screenshot.sound`); высоту окна
  сообщает `shots_hud_layout({ height })`, пустая стопка → `shots_hud_hide()`. Таймер — по
  `settings.screenshot.timeout` (0 — не скрывать), пауза на ховере/драге. Кнопки — §SPEC 9.4 через команды
  §18.10 и существующие `clip_copy`, `clips_delete`, `clip_ocr` + `text_copy`, `clip_drag_out`.
- На время оверлея окно QAO прячет и возвращает Rust (W6-B `hide_for_capture` / `restore_after_capture`,
  стопка сохраняется); сам QAO на `shot:session` ничего не показывает и не прячет, только ставит таймеры
  на паузу до `shot:end` (W6-E). `events::shot_session` пока шлёт только в `capture` — чтобы пауза
  работала, событие нужно и окну `shots` (сниппет в отчёте W6-E).
- Окно не активируется: никаких клавиш, фокус-кольцо не нужно; доступность — `aria-label` у кнопок.
- **Закреплённые окна**: каждое окно `pin-<n>` на старте зовёт `shot_pin_get()` (Rust знает метку окна
  → запись) и показывает `imageUrl` (`liq://media`). Драг — `startDragging()`; колесо — масштаб окна
  (`shot_pin_resize({ scale })` — Rust меняет размер вокруг курсора); `Ctrl+колесо` — прозрачность
  (CSS); двойной клик — 100 % ↔ исходный; `Esc`/× → `shot_pin_close()`; ПКМ — меню.

**Нативное (W6-B, `screenshot/qao.rs`, `pin.rs`).**
- **`shots`**: строится на своём потоке при первом захвате (`qao::hide_for_capture` — к коммиту области
  страница успевает загрузиться) или при первом `qao::show_added`; затем живёт скрытым. Свойства — §18.2
  (`focusable(false)` = `WS_EX_NOACTIVATE`, `focused(false)`, `disable_drag_drop_handler`,
  `WDA_EXCLUDEFROMCAPTURE`, `WS_EX_TOOLWINDOW`). Ширина — константа `QAO_WIDTH` = **296** логических px
  (карточка 248 + поле под тень; итог — по отчёту W6-E, меняется одна константа), высота — из
  `shots_hud_layout({height})`, не больше рабочей области минус отступы; угол — `settings.screenshot.corner`
  у **рабочей области** (`rcWork`, над панелью задач) монитора снимка (`ShotAdded.monitor` — имя устройства
  GDI; монитор отключён — монитор курсора), отступ `MARGIN` = 16 логических px. Окно показывается только
  после `shots_hud_layout` с высотой > 0 (размер известен) и без активации; `0` / `shots_hud_hide` — скрыть.
- **API для W6-A**: `qao::show_added(app, ShotAdded)` (вместо прямого `events::shot_added`: окно может ещё не
  существовать), `qao::hide_for_capture(app)` (начало любой сессии, в т. ч. без оверлея),
  `qao::restore_after_capture(app)` (конец сессии — коммит или отмена). Все — с любого потока, не блокируют.
- **Первое событие.** `shot:added` доходит только до подписавшейся страницы. Пока страница не загружена,
  payload ждёт в `QaoState.pending`; по `PageLoadEvent::Finished` (колбэк `on_page_load` строителя окна)
  через 400 мс (`FLUSH_DELAY`; страница подписывается при выполнении модуля, раньше `Finished`) очередь
  уходит событиями. Перезагрузка страницы (`Started`) снова копит события. Надёжнее — команда-«рукопожатие»
  `shots_hud_ready() -> ShotAdded[]` (предложение координатору, отчёт W6-B).
- **Временные файлы** (§18.4, снимок не в истории — `ShotAdded.tempPath` в `%TEMP%\Liqlip\shots\`): удаляются
  при пустой стопке (кроме выданных < 1,5 с назад), при выдаче пятого и далее (стопка держит 4), на старте —
  весь каталог (`windows::init` → поток). Удаляются только обычные файлы прямо в этом каталоге после
  `canonicalize` (тест), путь из payload сам по себе ничего не решает. Схемы для показа `tempPath` в карточке
  нет (маршрут `liq` — узел/W6-A; предложение — в отчёте W6-B).
- **`pin-<n>`**: `shot_pin(id)` / `pin::open` (редактор, коммит W6-A) — запись `image`/`screenshot` с размером,
  иначе `invalid`. Метки — растущий счётчик процесса (`pin-1`, `pin-2`, …, не повторяются); до `MAX_PINS` = 6,
  седьмое уничтожает самое старое. Окно: прозрачное, без рамки, always-on-top, skip-taskbar, focusable,
  `focused(false)`, `shadow(false)`, вне Alt+Tab, **не** исключено из захвата; показ без активации (клик
  активирует — тогда работают `Esc` и меню). Размер: картинка 1:1 (1 px картинки = 1 физ. px монитора — снимок
  выглядит как на экране), ужатая до 80 % монитора, плюс поле `BLEED` = **12 логических px** с каждой стороны
  под тень страницы (W6-E рисует внутри; начальный масштаб страница считает по размеру окна минус поле);
  по центру у курсора, внутри рабочей области. Запись заносится в `PinState` **до** построения — страница
  спрашивает `shot_pin_get` сразу при загрузке. `shot_pin_get` — по метке вызывающего окна (`imageUrl` —
  `liq://media/<id>?v=<n>`); `shot_pin_resize({scale})` — `scale` × 100 %, зажим 10–400 %, сторона ≤ 32 767 px,
  точка картинки под курсором остаётся на месте (тест); `shot_pin_close` — `destroy()` (только для `pin-*`).

#### 18.8.1. Фронт QAO — как сделано (W6-E)

Файлы: `src/windows/shots/` — `Shots.svelte` (корень: подписки, тик, мок-кнопки), `ShotCard.svelte`
(карточка), `qao.ts` (контроллер без рун: стопка, таймеры, кнопки, жизненный цикл drag-out; корень
передаёт `$state`-объект и настоящие команды, тест — обычный объект и подделки), `stack.ts` (чистая
модель + геометрия), `shutter.ts` (звук), `glyphs/pin.svelte` (свой глиф «кнопка-булавка» в формате
Heroicons — у Heroicons его нет). Тесты: `tests/shotsStack.test.mjs`, `tests/shotsQao.test.mjs`.

- **Геометрия окна (для W6-B).** Карточка 256 × 168 логических px: `raised`-сквиркл r16 с рамкой 4,
  внутри колодец картинки r12 (концентрично). Зазор 12. Вокруг стопки прозрачное поле `BLEED = 24`
  со всех сторон — поле тени `--shadow-pop` и одновременно визуальный отступ от краёв рабочей
  области. **Ширина окна фиксирована: 304** (`WINDOW_W`). **Высота** = `48 + n·168 + (n − 1)·12`
  (1 → 216, 2 → 396, 3 → 576, 4 → 756) — её QAO шлёт в `shots_hud_layout({height})`. Стопка
  прижата к низу окна, новая карточка — снизу: окно растёт вверх, поэтому W6-B держит **нижний край
  окна на нижнем краю рабочей области** и угол по `settings.screenshot.corner` **без своего отступа**
  (отступ 24 уже в поле окна; с отступом W6-B 16 видимый зазор станет 40). Высоту выше рабочей
  области W6-B зажимает — карточки тогда ужимаются (flex), не ниже 96.
- **Когда шлётся высота.** Рост — сразу при `shot:added` (в том числе при вытеснении, когда высота та
  же — W6-B может переставить окно к монитору нового снимка); уменьшение — через 220 мс (уход
  карточки 160 мс + сдвиг соседей 200 мс, чтобы верх не обрезался); пустая стопка → `shots_hud_hide()`
  после той же паузы.
- **Стопка** (`stack.ts`, основа — LT `stack.ts`): до 4, новая снизу, пятая вытесняет самую старую.
  Ключ карточки — `clip:<id>`, при `clip: null` — `tmp:<tempPath>`. Та же запись ещё раз (снимок,
  совпавший пиксель-в-пиксель, поднимает существующую запись, §18.5) — карточка переезжает вниз
  свежей. «Закрепить в стопке» из LT нет (П-16): «Закрепить» — на экран.
- **Таймер.** `timeout` берётся из настроек в момент снимка (0 — карточка не скрывается, полоски нет).
  Тик раз в 100 мс, только пока стопка не пуста; шаг ограничен 1 с (после сна карточки не исчезают
  разом). Пауза **всей** стопки: указатель над стопкой (карточка под курсором никогда не сдвинется из-за
  ухода соседки), drag-out, сессия захвата (`shot:session` → `shot:end`; `shot:added` тоже снимает
  паузу — на случай потерянного `shot:end`), `document.hidden`. Карточка с ошибкой сохранения не
  отсчитывает время — непрочитанная ошибка не исчезает сама. Полоска таймера — 3 px на собственной
  `raised`-капсуле 9 px (читается на любом снимке в обеих темах).
- **Кнопки** (на ховере; ink-капсула 40 px из кругов 32 — геометрия тоста; × — ink-круг 24 в левом
  верхнем углу на отступе 4): Копировать → `clip_copy {ids:[id], mode:'auto'}` · Редактировать →
  `shot_edit` · Закрепить → `shot_pin` · Сохранить → `shot_save` (путь → кнопка становится «Показать
  в папке» → `shot_reveal`; `not_found` у `shot_reveal` = файла нет → снова «Сохранить» + тост) ·
  Текст → `clip_ocr` → `text_copy` (недоступна с подсказкой, если OCR выключен в настройках или
  `ocr_status().available = false`) · Удалить → `clips_delete` (файл в папке не трогается) → карточка
  закрывается · × — закрыть. Клик по картинке — `shot_edit`. Одна команда за раз на карточку (кнопка
  показывает вращающийся `loaderCircle`). Ошибки — по коду (`not_found` → «Снимок удалён», `busy` →
  «Занято», иначе своя для действия).
- **Drag-out** (смысл тестов LT `hudLifecycle`): нажатие + сдвиг > 6 px → `clip_drag_out([id])`
  (захват указателя отпускается — мышью владеет OLE-цикл). `dropped` закрывает **только эту**
  карточку (новые, пришедшие во время drag, остаются); `cancelled`/ошибка — таймер карточки заново
  с полного, `hovered` сбрасывается (OLE-цикл съел `pointerleave`; следующее движение мыши взводит
  ховер снова); `pointercancel`/потеря захвата — не клик; отпускание без сдвига — клик → редактор.
- **Тосты — внутри карточки** (капсула 32 у нижнего края, над полоской; 1,6 с, ошибки 2,6 с):
  «Скопировано», «Текст скопирован», «Текст не найден», «Распознаю текст…», «Сохранено» и ошибки.
  «Не удалось сохранить · Изменить папку» (`shot:added.error = 'save_failed'` или отказ `shot_save`) —
  постоянная плашка `raised` r8 внизу карточки (концентрично колодцу), кнопка ведёт в
  `settings_open('screenshot')`.
- **`clips:changed`**: `deleted` с `ids` — карточки этих записей уходят; `updated` — карточка берёт
  `clip_detail` (новая миниатюра после правки, `meta.shot.savedPath`); `reset` и `deleted` без `ids` —
  перепроверка всех (`not_found` → карточка уходит).
- **`clip: null`** (тип «Скриншоты» выключен в хранении): превью нет (схема `liq` не отдаёт
  `tempPath`), карточка показывает «Не в истории» с подсказкой и умеет только закрываться — все
  команды контракта берут id записи. Нужен маршрут для временного файла и действия над ним — открытый
  вопрос (отчёт W6-E).
- **Превью**: `clip.thumbUrl`, без него — `clip_detail(id).imageUrl`; пока грузится — скелетон колодца,
  битая — глиф. Загрузка ключуется по URL (сброс эффектом мог прийти после `load` закэшированной
  картинки и оставить скелетон).
- **Звук** (`shutter.ts`): Web Audio API — два коротких шумовых щелчка через полосовой фильтр, ~60 мс,
  без файла; на каждое `shot:added` при `settings.screenshot.sound`. `AudioContext` без жеста
  пользователя может стартовать `suspended` (политика автозапуска Chromium); `resume()` пробуется,
  отказ тихий. Играет ли звук в WebView2 без жеста — проверка INT-5 (вариант: общий для всех окон
  `additional_browser_args` с `--autoplay-policy=no-user-gesture-required`, решает координатор).
- **Мок** (`/shots.html`, только вне Tauri): капсула «Мок» вверху слева — «Добавить снимок»
  (`shotCapture('screen')` → мок шлёт `shot:added`), «Снимок · ошибка сохранения», «Снимок · не в
  истории» (подменяют полезную нагрузку следующего `shot:added` у себя, мок не трогают).

#### 18.8.2. Фронт закреплённого окна — как сделано (W6-E)

Файлы: `src/windows/pin/Pin.svelte`, `pinMath.ts` (чистые функции; `tests/pinMath.test.mjs`).

- **Контракт масштаба с W6-B.** `shot_pin_resize({scale})` — **абсолютный** масштаб: физические
  px экрана на px картинки, 1 = 100 % = пиксель в пиксель (закреплённый снимок ложится 1:1 на то,
  с чего снят). Окно = `round(картинка × scale / масштаб монитора)` логических px + `PIN_BLEED = 24`
  с каждой стороны (поле тени `--shadow-lift`: в 24 px под картинкой < 1 % альфы), вокруг курсора,
  пределы 10–400 %. Начальный масштаб фронт выводит из размера окна
  (`(innerWidth − 48) · devicePixelRatio / width`, в пределах 2 % — 100 %); после ответа Rust (окно
  могло быть зажато) масштаб снова берётся из окна.
- **Колесо** — ×1,1 на щелчок (дельты в px/строках/страницах — MDN `WheelEvent.deltaMode`), переход
  через 100 % останавливается ровно на 100 %; запросы склеиваются — один `shot_pin_resize` в полёте,
  последний выигрывает. **Ctrl+колесо** — непрозрачность 20–100 % по 10 % (CSS, вместе с тенью;
  × и меню не тускнеют). Значение — капсула в центре на 0,9 с.
- **Перетаскивание** — `getCurrentWindow().startDragging()` на нажатии левой (`detail === 1`), как
  делает сам Tauri для `data-tauri-drag-region` (`tauri-2.11.2/src/window/scripts/drag.js`): там же
  двойной клик распознаётся как второе нажатие с `detail === 2` — так и здесь (100 % ↔ размер при
  открытии). Нажатие по ×, меню, заглушке — не драг.
- **× на ховере** (ink-круг 24 в правом верхнем углу на отступе 4; только если короткая сторона ≥ 56)
  и `Esc` → `shot_pin_close()`. Радиус картинки r16, у крошечной — меньше (`cornerRadius`).
- **ПКМ — меню** в геометрии меню шелфа (`raised` r22, padding 6, строки — `LayerItem` из
  `picker/overlays/kit`): Копировать · Редактировать · Сохранить как… · Закрыть (`Esc`). Меню не может
  выйти за своё окно, поэтому в маленьком окне оно становится капсулой кругов 32 (ряд, иначе
  столбец); место — у курсора с переворотом и зажимом (`placeMenu`). Клавиатура в меню — стрелки,
  Home/End, Enter/Space, Esc (по `KeyboardEvent.code`); закрывается кликом мимо, потерей фокуса окна,
  после действия. Тосты («Скопировано», «Сохранено», ошибки) — капсула внизу картинки.
- **Состояния**: загрузка — скелетон; `shot_pin_get() = null`, отказ или битая картинка — `raised`-плашка
  «Картинка недоступна» с кнопкой «Закрыть».
- **Браузер** (`/pin.html?clip=<id>`, мок): окна нет — картинка масштабируется внутри страницы
  (`fitScale`: 100 % или меньше, чтобы занять ≤ 80 % страницы), «Закрыть» прячет её.

### 18.9. Окно редактора (`src/windows/editor/`, W6-C; нативное — W6-B)

- На старте `shot_editor_get()` → `ShotEditorPayload`; базовая картинка — `baseUrl` (`liq://base/<id>`).
  `shot:editor` — смена записи в уже открытом окне (если нет несохранённых изменений; иначе окно само
  спрашивает и отвечает `shot_editor_switch(accept)`).
- Действия: Копировать (`shot_editor_commit` с `action:'copy'` — копирует сведённое, запись не
  меняет) · Закрепить · Сохранить как… · Готово (`action:'done'` → `store::shots::apply_edit` + буфер +
  перезапись `saved_path` → окно закрывается). Тело — то же кадрирование §18.4 (`ShotEditorCommit`).
- Закрытие с изменениями: окно перехватывает `onCloseRequested` → диалог (свой, внутри окна).

**Нативное (W6-B, `screenshot/editor.rs`).**
- **Окно**: одно, строится по требованию из `shot_edit` (async) — §18.2; обычное окно (системный заголовок,
  панель задач, Alt+Tab, тема из настроек), мин. 640×420; размер — картинка 1:1 + место под панели (`CHROME`
  48×136 логических px — раскладка W6-C, меняются две константы), ужатая до 80 % рабочей области монитора
  курсора, по центру (тест). Перед показом открытый шелф прячется (`picker::hide_now`: always-on-top закрыл бы
  редактор), затем `show` + `SetForegroundWindow` (право есть после клика в QAO/шелфе или хоткея; отказ —
  мигает кнопка на панели задач, ничего не форсируем). Заголовок «Имя — Liqlip», следует за языком
  (`settings::subscribe`). Имя записи: заголовок пользователя → имя сохранённого файла без расширения →
  «Снимок экрана 1920×1080» / «Картинка 800×600» (строки временно в `editor.rs`, ключи `i18n_native` — сниппет).
- **Состояние** (`EditorState`): `current` (запись окна), `previous` (до неотвеченной смены), `building`.
  `shot_editor_get` → `current` (`null` — окна нет); `baseUrl` — `liq://base/<id>?v=<растущий n>`; `width/height`
  — размер исходника (`doc.w/h`, без документа — записи); `doc` — `store::shots::doc` (шов, до INT-5 `null`);
  `dirtyGuard` — `false`.
- **Смена записи**: повторный `shot_edit(id)` при открытом окне — та же запись → окно вперёд; другая →
  `previous = current`, `current = id`, `shot:editor` с `dirtyGuard: true`, окно вперёд. Страница без изменений
  просто грузит запись (ответ не обязателен); с изменениями спрашивает и отвечает `shot_editor_switch(accept)`:
  `true` — смена подтверждена, `false` — `current` снова прежняя. `shot_edit` во время построения окна только
  меняет `current` (страница спросит `shot_editor_get` после загрузки).
- **Коммит** — `editor::commit(app, req, rgba)` (async; подключение вместо `commit::editor_commit` — сниппет
  `screenshot/mod.rs` в отчёте W6-B): картинка — сведённый RGBA тела (PNG), без пикселей — исходник
  `frame::serve("/base/<id>")`. `done` → `store::shots::apply_edit` → буфер, если `settings.screenshot.copy` →
  перезапись `saved_path` → окно уничтожается (если смена записи не ждёт ответа — тогда окно остаётся: страница
  сохраняла перед сменой). `copy` → только буфер. `save_as` → диалог для этих PNG-байт. `pin` →
  `pin::open(id)` — закрепляется **запись как есть** (несохранённые аннотации не видны — открытый вопрос в
  отчёте W6-B). Примитивы W6-A — швы `editor::seam` (`doc`, `apply_edit`, `copy_png`, `rewrite_saved`,
  `save_png_as`) с точными сигнатурами; до INT-5 они отвечают `unimplemented: … (W6-A)`.
- **`editor::after_commit(app, action)`** (хук узла W5-A после `commit::editor_commit`) — та же оконная часть
  для показываемой записи: `done` закрывает, `pin` закрепляет.

### 18.10. Контракт (W5-A вносит в §9 и код; агенты W6 только реализуют)

```ts
export type Kind = 'text' | 'link' | 'code' | 'color' | 'contact' | 'image' | 'file' | 'screenshot';
export type ShotMode = 'region' | 'screen' | 'window' | 'repeat' | 'text';
export type ShotAction = 'done' | 'copy' | 'save' | 'save_as' | 'pin' | 'text';
export interface MonitorFrame { index: number; id: string; x: number; y: number; width: number; height: number;
  scale: number; frameUrl: string }                          // физ. px виртуального экрана
export interface WinRect { x: number; y: number; w: number; h: number; children: WinRect[] }  // физ. px кадра монитора
export interface ShotSession { id: number; mode: 'region' | 'text'; monitor: MonitorFrame; windows: WinRect[];
  lastRegion: Rect | null; quick: boolean; magnifier: boolean; dimensions: boolean }
export interface ShotCommit { sessionId: number; rect: Rect; action: ShotAction; doc: AnnotDoc | null;
  width: number; height: number }                            // width/height RGBA тела (0 — тела нет)
export interface ShotCommitResult { clipId: number | null; savedPath: string | null; text: string | null }
export interface ShotEditorPayload { clipId: number; name: string; baseUrl: string; width: number; height: number;
  doc: AnnotDoc | null; dirtyGuard: boolean }
export interface ShotEditorCommit { clipId: number; action: 'done' | 'copy' | 'save_as' | 'pin'; doc: AnnotDoc;
  width: number; height: number }
export interface PinPayload { clipId: number; imageUrl: string; width: number; height: number }
export interface ShotStatus { snippingKey: boolean; hotkeyErrors: { field: string; code: 'taken' | 'invalid' }[] }
// ClipMeta += shot?: { savedPath: string | null; edited: boolean }
// Counts += screenshot: number
```

**Настройки** (`Settings`, аддитивно, `serde(default)`; версия остаётся 1):
```ts
hotkeys: { …; shotRegion: string; shotScreen: string; shotWindow: string; shotRepeat: string; shotText: string };
  // по умолчанию "PrtSc", "Ctrl + PrtSc", "", "", ""
screenshot: {
  quick: boolean;                 // false — снимать сразу при отпускании
  magnifier: boolean;             // true
  dimensions: boolean;            // true
  sound: boolean;                 // true
  copy: boolean;                  // true
  save: boolean;                  // true
  folder: string;                 // "" = %USERPROFILE%\Pictures\Liqlip (FOLDERID_Pictures)
  fileName: string;               // "Liqlip {date} {time}"
  format: 'png' | 'jpg';          // png
  jpgQuality: number;             // 90 (60–100)
  afterCapture: 'overlay' | 'editor' | 'none';   // overlay (QAO)
  corner: 'left' | 'right';       // left
  timeout: number;                // 10 (с; 0 = не скрывать)
};
// storage.types получает ключ screenshot (true) — Record<Kind, boolean>
```

**Команды** (модуль `screenshot`, кроме отмеченных; все, что касаются записи, принимают **id записи**, не путь):

| Команда | Аргументы → результат |
|---|---|
| `shot_capture` | `{ mode: ShotMode; delay?: number }` → `()` |
| `shot_session` | → `ShotSession \| null` |
| `shot_stage` | `sessionId, stage: 'select' \| 'frame'` → `()` |
| `shot_commit` | сырое тело §18.4 → `ShotCommitResult` |
| `shot_cancel` | `sessionId` → `()` |
| `shot_edit` | `id` → `()` (открыть окно редактора) |
| `shot_editor_get` | → `ShotEditorPayload \| null` |
| `shot_editor_switch` | `accept: boolean` → `()` |
| `shot_editor_commit` | сырое тело (то же кадрирование, JSON `ShotEditorCommit`) → `Clip` |
| `shot_pin` | `id` → `()` |
| `shot_pin_get` | → `PinPayload \| null` (по метке вызывающего окна) |
| `shot_pin_resize` | `scale: number` → `()` |
| `shot_pin_close` | → `()` |
| `shot_save` | `id` → `string` (путь; папка снимков, имя по шаблону) |
| `shot_save_as` | `id` → `string \| null` (диалог) |
| `shot_reveal` | `id` → `()` (Проводник с выделением `saved_path`) |
| `shots_hud_layout` | `{ height: number }` → `()` |
| `shots_hud_hide` | → `()` |
| `shot_status` | → `ShotStatus` |
| `open_windows_settings` | `page: 'keyboard'` → `()` (`ms-settings:easeofaccess-keyboard`) |

**События**:

| Событие | Payload | Кто → кому |
|---|---|---|
| `shot:session` | `ShotSession` | screenshot → capture (начало сессии, смена монитора) |
| `shot:end` | `{ sessionId: number }` | screenshot → capture, shots |
| `shot:added` | `{ clip: Clip \| null; savedPath: string \| null; tempPath: string \| null; monitor: string; error: 'save_failed' \| null }` | screenshot → shots |
| `shot:editor` | `ShotEditorPayload` | screenshot → editor |
| `hud:show` | `kind` += `'countdown' \| 'shot_failed' \| 'text_copied' \| 'text_empty'` | screenshot → hud |

**URI**: `/shot/<sessionId>/<monitorIndex>.bmp` · `/base/<clipId>?v=<n>` (§18.3).

**Мок** (`src/lib/ipc/mock/shots.ts`, W5-A): сессия с кадром — сгенерированная в браузере «фальшивая
рабочая поверхность» (canvas → blob URL: окна-прямоугольники с заголовками и текстом, 1920×1080, `scale`
из `?mock-scale=`), 4–6 окон в `windows` (одно с детьми), `shot_commit` добавляет запись `screenshot` в
мок-ленту и шлёт `shot:added`; `shot_editor_get` отдаёт картинку записи из мока; флаги `?mock-shot=text`
(режим OCR), `?mock-shot-quick=1`. Хосты окон открываются в браузере: `/capture.html`, `/editor.html?clip=<id>`,
`/shots.html` (кнопка «добавить снимок» в мок-режиме), `/pin.html?clip=<id>`.

**Мок — как сделано (W5-A).** `mock/shots.ts`: `shot_session` в браузере всегда отдаёт сессию (после
конца прошлой — новую, `shot:session`); кадр — canvas → PNG → `blob:` (тот же origin — canvas не
tainted; `blob:` в CSP не нужен: в браузере CSP Tauri не действует, в Tauri мок не используется),
1920×1080 × `?mock-scale` (1–4), пять окон с заголовками и строками, у «Browser» — две панели-ребёнка;
`windows` = `fakeDesktop(scale).windows` (чистая функция, тест). `shot_commit` разбирает тело тем же
`parseFrameBody`, проверяет id сессии и действие, зажимает `rect` (`clampRect`), картинка — сведённый
RGBA или кроп кадра; запись `screenshot` в мок-ленту (`clips:changed {origin:'capture', change:'added'}`)
с `meta.shot`, затем `shot:end` и — при `afterCapture: overlay` — `shot:added` (`clip: null` +
`tempPath`, если тип выключен в `storage.types`); `action: 'text'` — текст и HUD `text_copied` без
записи. `shot_capture`: `screen`/`window`/`repeat` (без прошлой области — как `region`) коммитят сразу,
`delay` шлёт `hud:show countdown` по секундам; повторный триггер при открытой сессии игнорируется.
Редактор: `shot_editor_get` — по `?clip=`, иначе последняя `shot_edit`, иначе новейшая картинка;
`shot_editor_commit {done}` хранит документ и первый исходник (`baseUrl`). Плюс флаги
`?mock-snipping=1` (`shot_status`: PrtSc у «Ножниц», `hotkeys.shotRegion` — `taken`) и
`?mock-shot-dialog=cancel`. Сид: до 8 картинок сида (через одну, новые первыми) превращаются в
`screenshot` со своей картинкой и `meta.shot` — число записей сида не меняется. Флаги описаны в шапке
`mock/bus.ts`. В node (без DOM) картинки — SVG data URL.

**Заглушки (W5-A → W6).** Команды — в `screenshot/mod.rs` (узел), тело каждой — один делегат. Агент
наполняет **функцию своего подмодуля**, сигнатуры не меняет (нужно иначе — в отчёт). Все `pub(crate)`,
`app: &AppHandle`, ошибки — `Err("unimplemented: <команда>")` до реализации:

| Подмодуль | Функции (W5-заглушки) | Агент |
|---|---|---|
| `session.rs` | `struct SessionState` (Default; поля — W6-A) · `init(app)` (пусто) · `capture(app, mode: ShotMode, delay: Option<u32>) -> Result<(), String>` · `current(app) -> Result<Option<ShotSession>, String>` · `stage(app, session_id: i64, stage: ShotStage) -> Result<(), String>` · `cancel(app, session_id: i64) -> Result<(), String>` · `on_capture_close(app)` (пусто) | W6-A |
| `commit.rs` | `commit(app, req: ShotCommit, rgba: Option<&[u8]>) -> Result<ShotCommitResult, String>` · `editor_commit(app, req: ShotEditorCommit, rgba: Option<&[u8]>) -> Result<Clip, String>` (пиксели — срез тела IPC, живёт до конца вызова; на другой поток — `to_vec()`) | W6-A |
| `files.rs` | `save(app, id: i64) -> Result<String, String>` · `async save_as(app: AppHandle, id: i64) -> Result<Option<String>, String>` · `reveal(app, id: i64) -> Result<(), String>` | W6-A (по §18.1; в диспетчере W6 «сохранение» записано за B — решает координатор) |
| | **W6-A, как сделано:** папка `screenshot.folder` или `<Pictures>\Liqlip` (`SHGetKnownFolderPath(FOLDERID_Pictures)`); имя — шаблон: `{date}` = `yyyy-MM-dd`, `{time}` = `HHmmss` (как имена «Ножниц», без `:`), `{app}`, `{n}` — наименьшее свободное ≥ 1; без `{n}` занятое имя получает ` (2)`, ` (3)`…; запрещённые символы Windows выбрасываются, пробелы схлопываются, хвостовые точки/пробелы срезаются, зарезервированные имена (`CON`, `COM1`…) получают `_`; ≤ 150 символов; имя занимается атомарно (`create_new`). `save` — `{date}`/`{time}` момента создания записи; `save_as` / `save_png_as` — диалог без родителя, формат по расширению; `reveal` — `reveal_item_in_dir` плагина `opener` (`SHOpenFolderAndSelectItems`, как `clip_reveal`), нет файла → `not_found` | |
| `frame.rs` | `serve(app, path: &str) -> Option<(String, Vec<u8>)>` (`None` → 404) | W6-A |
| `grab.rs` · `targets.rs` | пустые (только описание): делегатов команд нет | W6-A |
| `windows.rs` | `init(app)` (пусто) · `exclude_from_capture(win: &WebviewWindow) -> Result<(), String>` (**реализовано** W5) | W6-B |
| `qao.rs` | `struct QaoState` · `layout(app, height: f64) -> Result<(), String>` · `hide(app) -> Result<(), String>` | W6-B |
| `editor.rs` | `struct EditorState` · `open(app, id: i64) -> Result<(), String>` · `payload(app) -> Result<Option<ShotEditorPayload>, String>` · `switch(app, accept: bool) -> Result<(), String>` · `after_commit(app, action: ShotEditorAction)` (пусто; `done` → закрыть окно) | W6-B |
| `pin.rs` | `struct PinState` · `open(app, id: i64) -> Result<(), String>` · `payload(app, win: &WebviewWindow) -> Result<Option<PinPayload>, String>` · `resize(app, win, scale: f64) -> Result<(), String>` · `close(app, win) -> Result<(), String>` | W6-B |
| `status.rs` | `status(app) -> Result<ShotStatus, String>` · `open_windows_settings(app, page: WindowsSettingsPage) -> Result<(), String>` | W6-B |

Фронт: `src/windows/{capture,shots,editor,pin}/` — корни-заглушки (`<name>.ts` монтирует `<Name>.svelte`:
шрифты, `app.css`, `initAppearance()`, глотание клавиш браузера `isBrowserShortcut`; у `capture`/`shots`/`pin`
прозрачный фон) с одной строкой `<префикс>.soon` — W6-C/D/E заменяют содержимое корня. Обёртки команд —
`commands.ts` (`shotCapture` … `openWindowsSettings`, для тел — `shotCommit(commit, rgba?)` /
`shotEditorCommit(commit, rgba?)` через `frameBody`), подписки — `events.ts` (`onShotSession`,
`onShotEnd`, `onShotAdded`, `onShotEditor`; `onHudShow` с новыми видами). HUD-окно (`src/windows/hud/`,
владельца в W6 нет) уже показывает `countdown` / `shot_failed` / `text_copied` / `text_empty`
(ключи `hud.*`, иконки clock / warning / ocr / info); W6-F: `countdown` — крупная цифра вместо чипа.

**Где код уточняет §18.10** (подробно — §9.10): `Counts += screenshot` = `Counts.kinds.screenshot`;
`ShotStatus.hotkeyErrors` — `SettingsError[]`; `shot_capture` и `shots_hud_layout` — именованные
аргументы (`{mode, delay}`, `{height}`), а не объект-запрос; `serve_uri` отдаёт `(mime, bytes)`, ответ
строит `lib.rs`; после `shot_editor_commit` вызывается `editor::after_commit` (W6-B); права — отдельный
`capabilities/screenshot.json` (`pin-*`: `core:window:allow-start-dragging`; `editor`:
`core:window:allow-destroy` — `onCloseRequested` в `@tauri-apps/api` 2.11 сам зовёт `destroy()`, если
обработчик не отменил закрытие); таймер QAO — поле `timeout` (в §18.8 было `overlayTimeout`).

### 18.11. Хоткеи (W6-A)

`HotkeyAction` += `ShotRegion, ShotScreen, ShotWindow, ShotRepeat, ShotText` (слоты = поля `hotkeys.*`
по порядку; `combo::FIELDS`, `Slots<T> = [T; 9]`, фикстура, `logic.ts` W6-F). Голый `PrtSc` уже
маршрутизируется в `RegisterHotKey` (`combo.rs:56-61`). Действия → `screenshot::trigger(mode)` на
коротком потоке.

**`PrtSc` и «Ножницы»**: `status.rs` (W6-B) читает
`HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` (DWORD; нет значения — считать по
версии Windows: на Windows 11 22H2+ по умолчанию включено — W6-B сверяет по документации Microsoft и
фиксирует источник в отчёте). Реестр не пишем. Ошибки регистрации хоткеев — в `ShotStatus.hotkeyErrors`
и в `SettingsResult.errors` как у остальных полей.

**Как сделано (W6-A, 2026-11-27).** `combo::SLOTS = 9`, `FIELDS`/`strings` дополнены пятью полями
`hotkeys.shot*` в порядке `HotkeySettings`; `HOOK_TABLE` — `[const { None }; SLOTS]`; движок,
дубли, откаты и повторы работают по всем 9 слотам (тест `screenshot_slots_follow_the_settings_order`:
`PrtSc` и `Ctrl + PrtSc` — OS-хоткеи, сочетание, равное сочетанию шелфа, — `invalid`). Действие →
`screenshot::shot_capture { mode }` (`block_on` на потоке действия). Ошибки для `shot_status` —
`hotkeys::current_errors(app)` (§8). Файл `settings.rs` уже знал все 9 полей (`revert_hotkey`).

**Как сделано (W6-B, `screenshot/status.rs`).**
- `snippingKey`: значение есть → `≠ 0`. Значения нет (пользователь не трогал переключатель) — умолчание Windows.
  Источник — Microsoft, KB5027303 (27.06.2023, OS Build 22621.1928, Windows 11 22H2,
  https://support.microsoft.com/help/5027303), раздел Highlights: «This update changes the default print screen
  (prt scr) key behavior. Pressing the print screen key opens the Snipping Tool by default. You can turn off this
  setting from Settings > Accessibility > Keyboard. If you have previously changed this setting, Windows will
  preserve your preference.» Отсюда: **вкл.** для сборки 22621 с UBR ≥ 1928 и любой более новой (22631 23H2,
  26100 24H2, …); **выкл.** для Windows 11 21H2 (22000) и Windows 10 (заметки Microsoft о смене умолчания для
  Windows 10 не нашлось; переключатель там есть, по умолчанию выключен). Сборка и UBR —
  `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` (`CurrentBuildNumber`, `UBR`): API версий зависят от
  манифеста и UBR не дают. Чистая функция `snipping_default(build, ubr)` + тест; ещё один тест только читает
  реестр машины. Реестр не пишется.
- `hotkeyErrors`: отчёт движка для неизменённых настроек — `hotkeys::apply_settings(app, h, h)` (каждое
  желаемое, но не активное сочетание; при отложенной регистрации — одна повторная попытка), отфильтрованный по
  `hotkeys.shot*`. Пока W6-A не зарегистрировал 5 слотов, движок знает 4 и ошибок снимков не сообщает. Во время
  записи сочетания (все хоткеи приостановлены) движок считает все слоты «не активны» — `taken`; окно настроек
  в это время статус не спрашивает. Чище — отдельный `hotkeys::errors()` (предложение W6-A, отчёт W6-B).
- `open_windows_settings('keyboard')` → `ms-settings:easeofaccess-keyboard` (learn.microsoft.com «Launch Windows
  Settings», Ease of access → Keyboard) через Rust API `tauri-plugin-opener` (`open_url` → `ShellExecuteExW`;
  JS-область разрешений к Rust API не относится).

### 18.12. Проверка

Гейты прежние (§12): `pnpm check` 0/0 · `pnpm test` · `pnpm build` · `cargo check` · `cargo test`.
Агенты W6 нативное приложение не запускают; фронт проверяют в браузере на моке (`pnpm dev`, порт 1430)
и CDP-скриншотами. Координатор на интеграции запускает `pnpm tauri dev` и проверяет по логам старт,
регистрацию хоткеев и создание окон; захват, фокус, `PrtSc`, drag-out и DPI проверяет Eddy на
установленной сборке (чек-лист в отчёте интеграции).
