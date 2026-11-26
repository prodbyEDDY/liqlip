// Heroicons outline (MIT), bundled locally as tiny Svelte components in ./heroicons/
// (generated from @iconify-json/heroicons 1.2.3, same format as Liquid Tools: outline,
// stroke 1.5, 1em box, currentColor). Render through <Ico name="…" />.
// Add an icon: drop `<name>.svelte` into ./heroicons/ (same header + svg shape), add the
// import + key below, then `pnpm build` (the only gate that resolves these imports).
import type { Component } from "svelte";
import _search from "./heroicons/magnifying-glass.svelte";
import _copy from "./heroicons/document-duplicate.svelte";
import _paste from "./heroicons/clipboard-document.svelte";
import _clipboard from "./heroicons/clipboard.svelte";
import _clipboardList from "./heroicons/clipboard-document-list.svelte";
import _heart from "./heroicons/heart.svelte";
import _more from "./heroicons/ellipsis-horizontal.svelte";
import _folder from "./heroicons/folder.svelte";
import _folderOpen from "./heroicons/folder-open.svelte";
import _folderPlus from "./heroicons/folder-plus.svelte";
import _folderMinus from "./heroicons/folder-minus.svelte";
import _plus from "./heroicons/plus.svelte";
import _minus from "./heroicons/minus.svelte";
import _trash from "./heroicons/trash.svelte";
import _link from "./heroicons/link.svelte";
import _file from "./heroicons/document.svelte";
import _document from "./heroicons/document-text.svelte";
import _text from "./heroicons/bars-3-bottom-left.svelte";
import _image from "./heroicons/photo.svelte";
import _code from "./heroicons/code-bracket.svelte";
import _palette from "./heroicons/swatch.svelte";
import _mail from "./heroicons/envelope.svelte";
import _phone from "./heroicons/phone.svelte";
import _at from "./heroicons/at-symbol.svelte";
import _hash from "./heroicons/hashtag.svelte";
import _pause from "./heroicons/pause.svelte";
import _play from "./heroicons/play.svelte";
import _skip from "./heroicons/forward.svelte";
import _stack from "./heroicons/queue-list.svelte";
import _layers from "./heroicons/rectangle-stack.svelte";
import _eye from "./heroicons/eye.svelte";
import _eyeOff from "./heroicons/eye-slash.svelte";
import _pen from "./heroicons/pencil.svelte";
import _edit from "./heroicons/pencil-square.svelte";
import _external from "./heroicons/arrow-top-right-on-square.svelte";
import _settings from "./heroicons/cog-6-tooth.svelte";
import _keyboard from "./heroicons/command-line.svelte";
import _lock from "./heroicons/lock-closed.svelte";
import _shield from "./heroicons/shield-check.svelte";
import _ban from "./heroicons/no-symbol.svelte";
import _arrowUp from "./heroicons/arrow-up.svelte";
import _arrowDown from "./heroicons/arrow-down.svelte";
import _arrowLeft from "./heroicons/arrow-left.svelte";
import _arrowRight from "./heroicons/arrow-right.svelte";
import _arrowsUpDown from "./heroicons/arrows-up-down.svelte";
import _reverse from "./heroicons/arrows-right-left.svelte";
import _undo from "./heroicons/arrow-uturn-left.svelte";
import _loaderCircle from "./heroicons/arrow-path.svelte";
import _refresh from "./heroicons/arrow-path.svelte";
import _check from "./heroicons/check.svelte";
import _checkCircle from "./heroicons/check-circle.svelte";
import _chevronDown from "./heroicons/chevron-down.svelte";
import _chevronUp from "./heroicons/chevron-up.svelte";
import _chevronLeft from "./heroicons/chevron-left.svelte";
import _chevronRight from "./heroicons/chevron-right.svelte";
import _x from "./heroicons/x-mark.svelte";
import _info from "./heroicons/information-circle.svelte";
import _warning from "./heroicons/exclamation-triangle.svelte";
import _clock from "./heroicons/clock.svelte";
import _languages from "./heroicons/language.svelte";
import _monitor from "./heroicons/computer-desktop.svelte";
import _appWindow from "./heroicons/window.svelte";
import _pointer from "./heroicons/cursor-arrow-rays.svelte";
import _grid from "./heroicons/squares-2x2.svelte";
import _archive from "./heroicons/archive-box.svelte";
import _table from "./heroicons/table-cells.svelte";
import _video from "./heroicons/film.svelte";
import _music from "./heroicons/musical-note.svelte";
import _inbox from "./heroicons/inbox.svelte";
import _sparkles from "./heroicons/sparkles.svelte";
import _download from "./heroicons/arrow-down-tray.svelte";
import _upload from "./heroicons/arrow-up-tray.svelte";
import _database from "./heroicons/circle-stack.svelte";
import _power from "./heroicons/power.svelte";
import _sliders from "./heroicons/adjustments-horizontal.svelte";
import _ocr from "./heroicons/document-magnifying-glass.svelte";
import _camera from "./heroicons/camera.svelte";

export const ICONS = {
  search: _search,
  copy: _copy,
  paste: _paste,
  clipboard: _clipboard,
  clipboardList: _clipboardList,
  heart: _heart,
  more: _more,
  folder: _folder,
  folderOpen: _folderOpen,
  folderPlus: _folderPlus,
  folderMinus: _folderMinus,
  plus: _plus,
  minus: _minus,
  trash: _trash,
  link: _link,
  file: _file,
  document: _document,
  text: _text,
  image: _image,
  code: _code,
  palette: _palette,
  mail: _mail,
  phone: _phone,
  at: _at,
  hash: _hash,
  pause: _pause,
  play: _play,
  skip: _skip,
  stack: _stack,
  layers: _layers,
  eye: _eye,
  eyeOff: _eyeOff,
  pen: _pen,
  edit: _edit,
  external: _external,
  settings: _settings,
  keyboard: _keyboard,
  lock: _lock,
  shield: _shield,
  ban: _ban,
  arrowUp: _arrowUp,
  arrowDown: _arrowDown,
  arrowLeft: _arrowLeft,
  arrowRight: _arrowRight,
  arrowsUpDown: _arrowsUpDown,
  reverse: _reverse,
  undo: _undo,
  loaderCircle: _loaderCircle,
  refresh: _refresh,
  check: _check,
  checkCircle: _checkCircle,
  chevronDown: _chevronDown,
  chevronUp: _chevronUp,
  chevronLeft: _chevronLeft,
  chevronRight: _chevronRight,
  x: _x,
  info: _info,
  warning: _warning,
  clock: _clock,
  languages: _languages,
  monitor: _monitor,
  appWindow: _appWindow,
  pointer: _pointer,
  grid: _grid,
  archive: _archive,
  table: _table,
  video: _video,
  music: _music,
  inbox: _inbox,
  sparkles: _sparkles,
  download: _download,
  upload: _upload,
  database: _database,
  power: _power,
  sliders: _sliders,
  ocr: _ocr,
  camera: _camera,
} satisfies Record<string, Component>;
export type IconName = keyof typeof ICONS;
