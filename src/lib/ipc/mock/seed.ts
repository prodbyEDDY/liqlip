// Deterministic seed for the browser mock (owner: W1-E; W0 baseline): ~300 records of all
// seven kinds (EN/RU texts of different lengths, code, links with hosts, colors, emails /
// phones, generated SVG "images", files with many extensions), 4 folders, 6 snippets and
// a handful of source apps with placeholder icons.
import type { ClipMeta, Format, Kind, OcrState } from "../types";
import { rng } from "./bus";

/** Internal record: the union of Clip + ClipDetail fields plus store-only columns. */
export interface MockClip {
  id: number;
  kind: Kind;
  title: string | null;
  preview: string;
  text: string | null;
  appId: number | null;
  createdAt: number;
  copiedAt: number;
  copyCount: number;
  usedAt: number | null;
  useCount: number;
  favorite: boolean;
  favOrder: number | null;
  folderId: number | null;
  size: number;
  width: number | null;
  height: number | null;
  charCount: number | null;
  wordCount: number | null;
  lineCount: number | null;
  fileCount: number | null;
  files: { path: string; exists: boolean; size: number | null }[] | null;
  formats: Format[];
  meta: ClipMeta;
  ocr: OcrState;
  ocrText: string | null;
  thumbUrl: string | null;
  imageUrl: string | null;
  deletedAt: number | null;
}

export interface MockApp {
  id: number;
  exe: string;
  name: string;
  iconUrl: string | null;
}

export interface MockFolder {
  id: number;
  name: string;
  position: number;
  createdAt: number;
}

export interface MockSnippet {
  id: number;
  name: string;
  body: string;
  keyword: string | null;
  position: number;
  createdAt: number;
  updatedAt: number;
  useCount: number;
}

export interface MockDb {
  clips: MockClip[];
  apps: MockApp[];
  folders: MockFolder[];
  snippets: MockSnippet[];
  nextId: number;
}

// ── helpers ──────────────────────────────────────────────────────────────────

const svg = (body: string, w: number, h: number) =>
  "data:image/svg+xml;charset=utf-8," +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`);

/** Placeholder app icon: a grey squircle with the app's initial (monochrome chrome). */
function appIcon(letter: string, shade: string): string {
  return svg(
    `<rect x="2" y="2" width="60" height="60" rx="16" fill="${shade}"/>` +
      `<text x="32" y="42" font-family="Segoe UI, Arial" font-size="30" font-weight="700" fill="#fff" text-anchor="middle">${letter}</text>`,
    64,
    64,
  );
}

const utf8Len = (s: string) => new TextEncoder().encode(s).length;
export const metrics = (s: string) => ({
  charCount: [...s].length,
  wordCount: s.split(/\s+/).filter(Boolean).length,
  lineCount: s.length === 0 ? 0 : s.split(/\r?\n/).filter((l, i, a) => i < a.length - 1 || l !== "").length,
});
export const previewOf = (s: string) => [...s].slice(0, 512).join("");

// ── content pools ────────────────────────────────────────────────────────────

const APPS: [string, string, string][] = [
  ["c:\\program files\\google\\chrome\\application\\chrome.exe", "Google Chrome", "C"],
  ["c:\\users\\me\\appdata\\roaming\\telegram desktop\\telegram.exe", "Telegram", "T"],
  ["c:\\users\\me\\appdata\\local\\programs\\microsoft vs code\\code.exe", "Visual Studio Code", "V"],
  ["c:\\program files\\microsoft office\\root\\office16\\winword.exe", "Microsoft Word", "W"],
  ["c:\\program files\\microsoft office\\root\\office16\\excel.exe", "Microsoft Excel", "X"],
  ["c:\\windows\\explorer.exe", "Проводник", "П"],
  ["c:\\users\\me\\appdata\\local\\figma\\figma.exe", "Figma", "F"],
  ["c:\\program files\\windowsapps\\microsoft.windowsterminal\\windowsterminal.exe", "Windows Terminal", ">"],
  ["c:\\windows\\system32\\notepad.exe", "Блокнот", "Б"],
];
const SHADES = ["#2b2b2b", "#3a3a3a", "#4a4a4a", "#5a5a5a", "#1f1f1f", "#444", "#333", "#555", "#262626"];

const TEXTS = [
  "Meeting moved to Thursday, 3 pm.",
  "Встреча переносится на четверг, 15:00.",
  "Don't forget to send the invoice before Friday.",
  "Не забудь отправить счёт до пятницы.",
  "ok",
  "Спасибо!",
  "The quick brown fox jumps over the lazy dog.",
  "Съешь же ещё этих мягких французских булок, да выпей чаю.",
  "Order #48213 has shipped and should arrive within 3–5 business days. Tracking will be available once the carrier scans the parcel.",
  "Добрый день! Высылаю обновлённую смету по проекту. Изменения: добавлены работы по второму этажу, пересчитана стоимость материалов с учётом новых цен поставщика. Жду ваших комментариев до конца недели.",
  "Release notes 0.9\n\n• Faster search on large histories\n• Fixed paste into elevated windows\n• New: snippets with {date} placeholders\n• Russian layout shortcuts now work everywhere",
  "Список покупок:\n— молоко\n— хлеб\n— сыр\n— яблоки\n— кофе в зёрнах",
  "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.",
  "Адрес доставки: г. Казань, ул. Баумана, д. 12, кв. 34. Домофон 34К, этаж 5.",
  "TODO: rename the endpoint, update the docs, ping QA",
  "Пароль от Wi-Fi в переговорке: на стикере у входа",
  "Q3 revenue grew 18% quarter over quarter, driven mostly by the enterprise tier.",
  "Цитата дня: «Простота — это предельная сложность».",
];

const CODES = [
  "const total = items.reduce((sum, item) => sum + item.price * item.qty, 0);",
  'fn main() {\n    let name = std::env::args().nth(1).unwrap_or_else(|| "world".into());\n    println!("Hello, {name}!");\n}',
  "SELECT id, title, copied_at\nFROM clips\nWHERE deleted_at IS NULL\nORDER BY copied_at DESC\nLIMIT 40;",
  'def slugify(text: str) -> str:\n    text = re.sub(r"[^\\w\\s-]", "", text.lower())\n    return re.sub(r"[\\s_-]+", "-", text).strip("-")',
  '{\n  "name": "liqlip",\n  "version": "0.1.0",\n  "private": true\n}',
  "git rebase -i HEAD~3 && git push --force-with-lease",
  "export function clamp(v: number, lo: number, hi: number): number {\n  return Math.min(hi, Math.max(lo, v));\n}",
  ".card {\n  display: grid;\n  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));\n  gap: 12px;\n}",
  "docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=dev postgres:17",
  "<button class=\"btn btn--primary\" type=\"submit\">Save</button>",
];

const LINKS = [
  "https://github.com/tauri-apps/tauri/releases",
  "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
  "https://docs.rs/rusqlite/latest/rusqlite/",
  "https://ru.wikipedia.org/wiki/Буфер_обмена",
  "https://www.figma.com/file/AbC123/Liqlip-shelf",
  "https://learn.microsoft.com/windows/win32/dataxchg/clipboard",
  "https://svelte.dev/docs/svelte/$state",
  "https://news.ycombinator.com/item?id=41234567",
  "https://habr.com/ru/articles/812345/",
  "https://maps.google.com/?q=55.7558,37.6173",
  "https://tailwindcss.com/docs/theme",
  "www.example.com/pricing",
];

const COLORS: [string, string, string, string][] = [
  ["#FF5733", "#FF5733", "rgb(255, 87, 51)", "hsl(11, 100%, 60%)"],
  ["#1E1E1E", "#1E1E1E", "rgb(30, 30, 30)", "hsl(0, 0%, 12%)"],
  ["rgb(34, 197, 94)", "#22C55E", "rgb(34, 197, 94)", "hsl(142, 71%, 45%)"],
  ["#0EA5E9", "#0EA5E9", "rgb(14, 165, 233)", "hsl(199, 89%, 48%)"],
  ["hsl(48, 96%, 53%)", "#FACC15", "rgb(250, 204, 21)", "hsl(48, 96%, 53%)"],
  ["#F4F5F7", "#F4F5F7", "rgb(244, 245, 247)", "hsl(220, 14%, 96%)"],
  ["#8B5CF6CC", "#8B5CF6", "rgb(139, 92, 246)", "hsl(258, 90%, 66%)"],
  ["#fff", "#FFFFFF", "rgb(255, 255, 255)", "hsl(0, 0%, 100%)"],
];

const CONTACTS: [string, "email" | "phone"][] = [
  ["anna.petrova@example.com", "email"],
  ["support@liqlip.app", "email"],
  ["john.doe@company.io", "email"],
  ["+7 912 345-67-89", "phone"],
  ["+1 (415) 555-0132", "phone"],
  ["8 800 555-35-35", "phone"],
];

const FILE_SETS: string[][] = [
  ["C:\\Users\\me\\Documents\\Report Q3.pdf"],
  ["C:\\Users\\me\\Documents\\Договор поставки.docx"],
  ["C:\\Users\\me\\Downloads\\budget-2026.xlsx"],
  ["C:\\Users\\me\\Downloads\\assets.zip"],
  ["C:\\Users\\me\\Pictures\\Screenshots\\Screenshot 2026-11-20 101544.png"],
  ["C:\\Users\\me\\Videos\\demo.mp4"],
  ["C:\\Users\\me\\Music\\track01.mp3"],
  ["C:\\Projects\\liqlip\\README.md"],
  ["C:\\Projects\\design"],
  [
    "C:\\Users\\me\\Pictures\\Trip\\IMG_0012.jpg",
    "C:\\Users\\me\\Pictures\\Trip\\IMG_0013.jpg",
    "C:\\Users\\me\\Pictures\\Trip\\IMG_0014.jpg",
  ],
  ["C:\\Users\\me\\Desktop\\notes.txt", "C:\\Users\\me\\Desktop\\todo.txt"],
];

const OCR_TEXTS = [
  "Total due: $1,240.00\nInvoice #2026-118",
  "Ошибка 0x80070005: отказано в доступе",
  "Build succeeded · 0 warnings · 12.4 s",
];

function imageSvg(r: () => number, w: number, h: number): string {
  const hue = Math.floor(r() * 360);
  const hue2 = (hue + 40 + Math.floor(r() * 80)) % 360;
  const blocks = Array.from({ length: 4 }, (_, i) => {
    const x = Math.floor(r() * w * 0.7);
    const y = Math.floor(r() * h * 0.7);
    return `<rect x="${x}" y="${y}" width="${Math.floor(w * (0.15 + r() * 0.3))}" height="${Math.floor(h * (0.1 + r() * 0.25))}" rx="${8 + i * 4}" fill="hsl(${(hue + i * 30) % 360} 60% ${55 + i * 8}%)" opacity="0.85"/>`;
  }).join("");
  return svg(
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue} 55% 45%)"/><stop offset="1" stop-color="hsl(${hue2} 60% 30%)"/></linearGradient></defs>` +
      `<rect width="${w}" height="${h}" fill="url(#g)"/>${blocks}`,
    w,
    h,
  );
}

const IMAGE_SIZES: [number, number][] = [
  [1920, 1080],
  [1080, 1920],
  [1200, 1200],
  [2560, 1440],
  [640, 480],
  [64, 64],
];

function hostOf(url: string): string {
  try {
    return new URL(url.startsWith("http") ? url : `https://${url}`).host;
  } catch {
    return "";
  }
}

function extOf(path: string): string | undefined {
  const name = path.split("\\").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : undefined;
}

// ── seed ─────────────────────────────────────────────────────────────────────

const KIND_WEIGHTS: [Kind, number][] = [
  ["text", 30],
  ["link", 15],
  ["code", 12],
  ["color", 8],
  ["contact", 7],
  ["image", 15],
  ["file", 13],
];

function pickKind(r: () => number): Kind {
  let x = r() * 100;
  for (const [k, w] of KIND_WEIGHTS) {
    if ((x -= w) < 0) return k;
  }
  return "text";
}

const DAY = 86_400_000;

/** Build one record of `kind` (also used by the live-capture simulator). */
export function makeClip(id: number, kind: Kind, r: () => number, copiedAt: number, appId: number | null): MockClip {
  const base: MockClip = {
    id,
    kind,
    title: null,
    preview: "",
    text: null,
    appId,
    createdAt: copiedAt,
    copiedAt,
    copyCount: 1,
    usedAt: null,
    useCount: 0,
    favorite: false,
    favOrder: null,
    folderId: null,
    size: 0,
    width: null,
    height: null,
    charCount: null,
    wordCount: null,
    lineCount: null,
    fileCount: null,
    files: null,
    formats: ["text"],
    meta: {},
    ocr: "none",
    ocrText: null,
    thumbUrl: null,
    imageUrl: null,
    deletedAt: null,
  };
  const pick = <T,>(arr: T[]) => arr[Math.floor(r() * arr.length)];
  const setText = (t: string) => {
    base.text = t;
    base.preview = previewOf(t);
    Object.assign(base, metrics(t));
    base.size = utf8Len(t);
  };
  switch (kind) {
    case "text": {
      setText(pick(TEXTS));
      if (r() < 0.25) {
        base.formats = ["text", "html"];
        base.size += 400;
      }
      if (r() < 0.08) base.formats.push("rtf");
      break;
    }
    case "code":
      setText(pick(CODES));
      break;
    case "link": {
      const url = pick(LINKS);
      setText(url);
      base.meta = { url: url.startsWith("http") ? url : `https://${url}`, host: hostOf(url) };
      break;
    }
    case "color": {
      const [raw, hex, rgb, hsl] = pick(COLORS);
      setText(raw);
      base.meta = { color: { hex, rgb, hsl } };
      break;
    }
    case "contact": {
      const [value, contact] = pick(CONTACTS);
      setText(value);
      base.meta = { contact };
      break;
    }
    case "image": {
      const [w, h] = pick(IMAGE_SIZES);
      const url = imageSvg(r, Math.round(w / 8), Math.round(h / 8));
      base.width = w;
      base.height = h;
      base.formats = ["png"];
      base.size = Math.floor(40_000 + r() * 2_000_000);
      base.thumbUrl = url;
      base.imageUrl = url;
      const o = r();
      if (o < 0.25) {
        base.ocr = "done";
        base.ocrText = pick(OCR_TEXTS);
      } else if (o < 0.3) {
        base.ocr = "pending";
      }
      break;
    }
    case "file": {
      const paths = pick(FILE_SETS);
      const text = paths.join("\n");
      base.text = text;
      base.preview = paths[0].split("\\").pop() ?? paths[0];
      base.fileCount = paths.length;
      base.files = paths.map((path) => ({ path, exists: r() > 0.15, size: Math.floor(r() * 5_000_000) }));
      base.formats = ["files", "text"];
      base.size = utf8Len(text);
      const ext = extOf(paths[0]);
      base.meta = ext ? { ext } : {};
      break;
    }
  }
  return base;
}

export function seed(opts: { size: number; empty: boolean }): MockDb {
  const r = rng(20261125);
  const t0 = Date.now();
  const apps: MockApp[] = opts.empty
    ? []
    : APPS.map(([exe, name, letter], i) => ({ id: i + 1, exe, name, iconUrl: appIcon(letter, SHADES[i % SHADES.length]) }));
  const folders: MockFolder[] = opts.empty
    ? []
    : ["Работа", "Design", "Код", "Личное"].map((name, i) => ({ id: i + 1, name, position: i + 1, createdAt: t0 - (30 - i) * DAY }));
  const snippets: MockSnippet[] = opts.empty
    ? []
    : [
        ["Signature", "Best regards,\nAlex"],
        ["Подпись", "С уважением,\nАлексей"],
        ["Meeting notes", "Meeting {date format=\"yyyy-MM-dd\"} — {time}\n\nAttendees:\nAgenda:\n{cursor}"],
        ["SQL: recent", "SELECT * FROM clips WHERE copied_at > {date offset=\"-7d\"} ORDER BY copied_at DESC;"],
        ["Ответ клиенту", "Здравствуйте!\nСпасибо за обращение. {cursor}\n\nС уважением, команда поддержки"],
        ["New UUID", "{uuid}"],
      ].map(([name, body], i) => ({
        id: i + 1,
        name,
        body,
        keyword: null,
        position: i + 1,
        createdAt: t0 - (20 - i) * DAY,
        updatedAt: t0 - (10 - i) * DAY,
        useCount: Math.floor(r() * 12),
      }));

  const clips: MockClip[] = [];
  const n = opts.empty ? 0 : opts.size;
  let ts = t0 - 60_000;
  for (let i = 0; i < n; i++) {
    // Denser recently: gaps grow with age.
    ts -= Math.floor(30_000 + r() * 60_000 * (1 + i / 12));
    const kind = pickKind(r);
    const appId = apps.length && r() > 0.08 ? apps[Math.floor(r() * apps.length)].id : null;
    const c = makeClip(n - i, kind, r, ts, appId);
    c.createdAt = ts - (r() < 0.2 ? Math.floor(r() * 10 * DAY) : 0);
    c.copyCount = r() < 0.2 ? 2 + Math.floor(r() * 4) : 1;
    if (r() < 0.3) {
      c.useCount = 1 + Math.floor(r() * 6);
      c.usedAt = ts + Math.floor(r() * 3_600_000);
    }
    if (r() < 0.08) {
      c.favorite = true;
      c.favOrder = clips.filter((x) => x.favorite).length + 1;
    }
    if (folders.length && r() < 0.12) c.folderId = folders[Math.floor(r() * folders.length)].id;
    if (r() < 0.04 && c.kind !== "image") c.title = ["Важное", "Template", "For later", "Ключ API тестовый"][Math.floor(r() * 4)];
    clips.push(c);
  }
  return { clips, apps, folders, snippets, nextId: n + 1 };
}
