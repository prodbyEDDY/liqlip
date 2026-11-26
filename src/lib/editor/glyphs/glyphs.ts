// The editor's own glyphs (ARCHITECTURE §18.6): Heroicons has no rectangle, ellipse, marker or
// arrow tool, and Liquid Tools reused one icon for several tools (pen = marker, effect = lasso,
// text = "hamburger" — research П-7). Same format as `src/lib/heroicons/`: 24 × 24, outline,
// stroke 1.5, round caps and joins, `currentColor`. Every tool has its own shape.
// `help` is Heroicons `question-mark-circle` (MIT, Tailwind Labs); the rest are drawn for Liqlip.

export const GLYPHS = {
  select: '<path d="M5.5 3.5 18.5 13l-5.9.9 3.2 6.1-2.6 1.3-3.1-6.1-4.6 4z"/>',
  arrow: '<path d="M5 19 19 5M10 5h9v9"/>',
  line: '<path d="M5 19 19 5"/>',
  rect: '<rect x="3.75" y="5.75" width="16.5" height="12.5" rx="2"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="8.25" ry="6.25"/>',
  pen: '<path d="M3.75 16.5c1.6-4.5 3.2-6.75 4.9-6.75 2.6 0 1.9 6 4.6 6 1.9 0 3.4-3.6 7-9.75"/>',
  marker:
    '<path d="m10.5 13.5 7-7a1.5 1.5 0 0 1 2.12 0l.88.88a1.5 1.5 0 0 1 0 2.12l-7 7M10.5 13.5l-2.25 2.25 3 3 2.25-2.25M8.25 15.75 4.5 19.5H9l1.5-1.5"/>',
  text: '<path d="M5.25 7.5V5.25h13.5V7.5M12 5.25v13.5M9.75 18.75h4.5"/>',
  counter: '<circle cx="12" cy="12" r="8.25"/><path d="m10.25 9.75 2-1.5v7.5"/>',
  redact: '<rect x="3.75" y="7.5" width="16.5" height="9" rx="1.5"/><path d="m7.5 16.5 4.5-9M12.75 16.5l4.5-9"/>',
  spotlight: '<circle cx="12" cy="12" r="4"/><path d="M12 3.75V6M12 18v2.25M3.75 12H6M18 12h2.25M6.2 6.2l1.4 1.4M16.4 16.4l1.4 1.4M6.2 17.8l1.4-1.4M16.4 7.6l1.4-1.4"/>',
  crop: '<path d="M6.75 3v14.25H21M3 6.75h14.25V21"/>',
  eyedropper:
    '<path d="m14.25 5.25 1.5-1.5a2.12 2.12 0 0 1 3 3l-1.5 1.5M12.75 6.75l4.5 4.5M15.75 9.75 7.5 18l-3 1.5 1.5-3 8.25-8.25"/>',
  undo: '<path d="M9 15 3 9m0 0 6-6M3 9h12a6 6 0 0 1 0 12h-3"/>',
  redo: '<path d="m15 15 6-6m0 0-6-6m6 6H9a6 6 0 0 0 0 12h3"/>',
  pin: '<path d="m15.75 3.75 4.5 4.5M14.25 5.25l-4.5 4.5-3.75.75-1.5 1.5 7.5 7.5 1.5-1.5.75-3.75 4.5-4.5M9 15l-5.25 5.25"/>',
  arrowStraight: '<path d="M5 19 19 5M10 5h9v9"/>',
  arrowCurved: '<path d="M4.5 19.5C6 11.5 11 6.75 19.5 5.25M14.25 3.75l5.25 1.5-2.25 4.75"/>',
  arrowDouble: '<path d="M6.5 17.5 17.5 6.5M11 6h7v7M13 18H6v-7"/>',
  textPlain: '<path d="M5.25 7.5V5.25h13.5V7.5M12 5.25v13.5M9.75 18.75h4.5"/>',
  textBadge: '<rect x="3.75" y="5.25" width="16.5" height="13.5" rx="3"/><path d="M8.25 9h7.5M12 9v6.75"/>',
  redactSolid: '<rect x="3.75" y="7.5" width="16.5" height="9" rx="1.5"/><path d="m7.5 16.5 4.5-9M12.75 16.5l4.5-9"/>',
  redactPixelate: '<rect x="3.75" y="3.75" width="16.5" height="16.5" rx="1.5"/><path d="M9.25 3.75v16.5M14.75 3.75v16.5M3.75 9.25h16.5M3.75 14.75h16.5"/>',
  redactBlur: '<path d="M12 3.75c3.5 4.25 5.25 7.25 5.25 9.75a5.25 5.25 0 0 1-10.5 0c0-2.5 1.75-5.5 5.25-9.75z"/>',
  shapeRect: '<rect x="3.75" y="5.75" width="16.5" height="12.5" rx="2"/>',
  shapeEllipse: '<ellipse cx="12" cy="12" rx="8.25" ry="6.25"/>',
  outline: '<rect x="4.5" y="4.5" width="15" height="15" rx="2.25"/>',
  fill: '<rect x="4.5" y="4.5" width="15" height="15" rx="2.25"/><path d="m4.75 11.5 6.75-6.75M4.75 17.5 17.5 4.75M10.5 19.25l8.75-8.75M16.5 19.25l2.75-2.75"/>',
  cornerRound: '<path d="M4.5 19.5V12a7.5 7.5 0 0 1 7.5-7.5h7.5"/>',
  cornerSquare: '<path d="M4.5 19.5v-15h15"/>',
  help:
    '<path d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 5.25h.008v.008H12v-.008Z"/>',
  zoomIn: '<circle cx="10.5" cy="10.5" r="6.75"/><path d="m15.5 15.5 4.75 4.75M10.5 8v5M8 10.5h5"/>',
  zoomOut: '<circle cx="10.5" cy="10.5" r="6.75"/><path d="m15.5 15.5 4.75 4.75M8 10.5h5"/>',
} as const;

export type GlyphName = keyof typeof GLYPHS;
