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

> Замер §4: 2026-11-26 (W1-A: хранилище реализовано целиком — `src-tauri/src/store/`;
> `cargo test` 76/76, из них 53 теста хранилища, включая проверку планов запросов; нативно не
> запускалось, стенда на 50 000 записей нет — решение Eddy).

### 4.1. Файлы

```
%APPDATA%\com.liqlip.app\
  liqlip.db (+ -wal, -shm)     SQLite (WAL)
  liqlip.db.check              маркер: в работе встретился SQLITE_CORRUPT → на следующем старте quick_check
  liqlip.db.corrupt-<ms>       битая база, отодвинутая при открытии (+ media.corrupt-<ms>, icons.corrupt-<ms>)
  media\<id>.png               полноразмерные картинки и PNG-альтернатива текстовых записей (Excel)
  media\thumbs\<id>.png        превью ≤ 360 px по длинной стороне (только image)
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
  (`INSERT … ON CONFLICT(exe) DO UPDATE … RETURNING`; непустое имя из захвата обновляет старое,
  пустое — оставляет; новое без имени — имя файла без `.exe`); запись с тем же `hash` (в том числе мягко
  удалённая) → `copied_at = max(copied_at, new)`, `copy_count + 1`, `deleted_at = NULL`, источник —
  последний, `kind` — новая классификация, `meta` — слияние полей (новое побеждает, где оно есть);
  богатые форматы новой копии заменяют одноимённые старые; файлы картинки дописываются, только если
  их нет. Иначе новая строка. PNG и превью пишутся **до** захвата блокировки во временный файл
  `media\.staging-*` и в транзакции переименовываются в `<id>.png` (большая картинка не держит БД);
  сбой → откат + удаление записанного. `size` пересчитывается из того, что реально сохранено.
  Валидация: пустой текст / картинка без валидного PNG / файлы без путей / пустой хэш → `invalid`;
  картинка > 64 МП → `invalid`; html/rtf > 2 МБ — отбрасываются (запись сохраняется); размеры
  картинки берутся из заголовка PNG. После вставки — сигнал фоновому потоку (ретенция с порогом).
- **Мягкое удаление** (`clips_delete`, всё или ничего): `deleted_at = now`; все листинги, `liq`-схема,
  `get_for_paste`, детали фильтруют `deleted_at IS NULL`; фоновая чистка удаляет строки старше
  2 минут жёстко вместе с файлами.
- **`undoToken`** — непрозрачная строка-ключ (`u<seq>.<ms>`); данные (id, тип, прежние значения)
  в памяти, TTL 2 мин, токен срабатывает один раз. Операции: удаление и «Очистить историю»
  (возврат строк, ещё помеченных **этим** `deleted_at`), перенос в папку (прежняя папка каждой
  записи; удалённая с тех пор папка → без папки), удаление папки (строка папки с тем же id; если имя
  занято — «Имя 2»; участники возвращаются в неё, при `withClips` — и из удаления). Событие отмены:
  удаление → `added`, папка → `updated`, очистка → `reset`.
- **Удалить недавнее** (`clips_delete_recent`, трей): жёстко, без undo, всё с `copied_at ≥ now − N мин`
  — избранное, папки и уже мягко удалённые тоже (подтверждает UI). Приватное действие: на его время
  `secure_delete = ON`, затем `PRAGMA incremental_vacuum` (до конца — по одной странице за шаг) и
  `PRAGMA wal_checkpoint(TRUNCATE)`; FTS — с `secure-delete`, так что токены пароля не остаются в
  индексе.
- **Очистить**: `history` — мягко всё, кроме избранного и папок (undo, событие `reset`); `all` — жёстко
  все записи и папки (сниппеты и `apps` остаются), FTS очищается одной командой `'delete-all'` (триггер
  удаления снимается и возвращается в той же транзакции), файлы удаляются, токены отмены сбрасываются,
  приватный режим как у «удалить недавнее»; `undoToken: null`.
- **Ретенция** (`lifecycle.rs::run_retention`): только «история» — живые записи с
  `favorite = 0 AND folder_id IS NULL` (избранное, папки, сниппеты не трогаются **и не считаются** в
  лимиты); по возрасту (`copied_at`; месяцы = 30 дней: 1d/1w/1m/3m/6m/1y = 1/7/30/90/180/365 дней) →
  оставить новейшие `maxEntries` → удалять старейшие, пока сумма `size` истории не влезет в
  `maxSizeMb`. Жёстко с файлами, пачками по 500 строк на транзакцию (захват и UI не ждут). Запуск: на
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
  следующий старт делает `quick_check`. База **новее** сборки (`user_version > 1`) не открывается
  (`failed: …`, старт прерывается) — данные не уничтожаются.

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
  конца); пустые и повторы (без учёта регистра) отбрасываются, не больше 32 термов. **Все** термы
  должны совпасть (AND), каждый — как подстрока.
- Терм ≥ 3 символов → `clips_fts MATCH` (title, text, ocr_text — полный текст, а не превью): каждый терм —
  строка FTS5 в кавычках с удвоением `"` (fts5.html §3.1), так что `AND`, `NEAR`, `*`, `:`, `(`, `-`
  пользователя — просто символы. trigram регистронезависим и для кириллицы (зонд W0 + тесты).
- Терм < 3 символов trigram обслужить не может (fts5.html §4.3.5) → `liq_icontains(preview|title, терм)`:
  своя SQL-функция (rusqlite `functions`), регистронезависимая по Unicode (`LIKE`/`lower()` SQLite
  сворачивают только ASCII). Ищет по превью (≤ 512 символов) и заголовку.
- `exclude` — те же правила: длинные одним `NOT IN (… MATCH "a" OR "b")`, короткие — `NOT liq_icontains`.
- Сниппеты (`snippets_list`) — то же по `snippets_fts` (name, body), короткие термы — по name/body.

### 4.5. Медиа и схема `liq`

`store::serve_uri(app, path)` (`media.rs::serve_path`): `/thumb/<id>` — только живая запись `image`;
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
(файлы `media`/`thumbs`/`icons` без строки, `.staging-*` старше часа; каталоги читаются **до** запроса
к БД, поэтому файл свежей вставки не удаляется).

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
> запускалось — живая проверка по чек-листу в отчёте W1-B).

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
           ДО копирования, > 64 МП не копируется
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
буфер для тестов), `win32.rs` (реализация шва, владелец, FileDescription), `pipeline.rs` (capture-цикл,
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
  (и `PGP PRIVATE KEY BLOCK`) с совпадающим `END`.
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
  ближайшее внешнее изменение (свои записи и дубли не в счёт). Каждое изменение состояния →
  `events::capture_status` + `tray::refresh`.
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
Win32-обвязки). Контракт IPC (§9) не менялся.

**Тесты** (`cargo test capture::`, 67): классификация (цвета/ссылки/контакты/код/края), секреты
(карты/ключи/PEM/ложные), CF_HTML и заголовки, состояние (пауза, таймер, пропуск, свои номера),
снимок на фейковом буфере (один OpenClipboard, только нужные форматы, приоритеты, маркеры — 7 случаев
вкл. Bitwarden и битый DWORD, игнор-лист, свой процесс, занятый буфер, лимиты, зависший формат и
зависший маркер), worker (`build_clip`: поля, секреты, тумблеры, усечение, Excel текст+PNG, «Копировать
изображение», дедуп PNG≡DIB, файлы), конвейер (старт, дубли событий, self-copy, пауза, пропуск,
режим сбора, повторы при занятом буфере и неотвечающем владельце, игнор/маркеры, зависший рендер,
поток событий, self-copy ровно по номеру), Win32-обёртки (зарегистрированные форматы, свой exe,
FileDescription, иконка explorer.exe 64×64, неблокирующий `app_icon_data_url` < 5 мс на промахе и
фоновое извлечение, listener — `WM_CLIPBOARDUPDATE`, отправленное самому себе, и остановка,
`apps_running`), **стенд** `bench_10_copies_per_second_for_10_seconds`: 100 записей за 10 с из
другого потока → 100/100 по порядку, все хэши разные, задержка p50 38 мс / max 47 мс (норма SPEC §7 —
≤ 300 мс).

**Не проверено нативно (риски):** реальные `WM_RENDERFORMAT`-задержки Office и поведение
`SMTO_ABORTIFHUNG` с занятыми, но живыми источниками; точное поведение номера последовательности
при рендере (документировано, что он растёт); маски CF_DIBV5 от разных писателей.

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
`/thumb/<id>?v=<n>` · `/media/<id>?v=<n>` · `/icon/<appId>?v=<n>` (версия у всех трёх — W1-A: после
пересоздания базы id могут совпасть, и без версии WebView2 показал бы чужую картинку из кэша) —
чтение файла в `spawn_blocking`,
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
pub fn app_icon_data_url(exe: &str) -> Option<String>;           // ← paste (TargetInfo.iconUrl); не блокирует: промах → None + фоновое извлечение
pub mod classify { pub fn classify_text(text: &str) -> (Kind, ClipMeta); }   // ← store
// добавлено W1-B (аддитивно):
pub fn stop();                                                     // ← lib.rs RunEvent::Exit: снять listener, остановить потоки
pub fn set_collect(app: &AppHandle, on: bool) -> Result<CaptureStatus, String>; // режим сбора в очередь
pub fn running_apps() -> Vec<RunningApp>;                          // ← apps_running (blocking-пул)
pub fn clipboard_sequence() -> u32;                                // GetClipboardSequenceNumber для mark_own_write
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
