// The Solite mark, defined once. SoliteLogo.tsx renders it and the favicon
// (src/assets/favicon.svg) repeats its SMALL weight, kept in step by logo-sync.test.ts.
// Derived from Sowel's ui/src/lib/logo.ts, commit 5da84dc1 (Sowel spec 210): the house, the
// eyes, the wink and the smile are Sowel's, number for number; the antenna on the roof is
// Solite's (the box listens to the radio). Round caps and joins.

export const VIEW_BOX = "20 -12 160 187";
/** The mark's width over its height, for a caller that sets one of the two. */
export const MARK_RATIO = 160 / 187;

export const HOUSE =
  "M100 30 L160 90 Q165 95 160 100 L160 150 Q160 158 152 158 L48 158 Q40 158 40 150 L40 100 Q35 95 40 90 Z";

export interface LogoWeight {
  /** Stroke widths: outline, eyes, smile. */
  house: number;
  eye: number;
  smile: number;
  /** Absolute paths, as the static SVG copies draw them. */
  leftEye: string;
  rightEye: string;
  smilePath: string;
  /** The winking eye, drawn around its own centre so it can scale in place. */
  wink: { origin: string; path: string };
  /** Solite's antenna: a mast from the ridge (ink), a ball and one wave per side (mid blue). */
  antenna: {
    mast: { path: string; width: number };
    ball: { cx: number; cy: number; r: number };
    waves: { paths: string[]; width: number };
  };
}

/** From 32 px up. */
export const MASTER: LogoWeight = {
  house: 9,
  eye: 7,
  smile: 7.5,
  leftEye: "M75 94 Q81 84 87 94",
  rightEye: "M113 94 Q119 84 125 94",
  smilePath: "M72 114 Q100 142 128 114",
  wink: { origin: "119 89", path: "M-6 5 Q0 -5 6 5" },
  antenna: {
    mast: { path: "M100 30 L100 14", width: 7 },
    ball: { cx: 100, cy: 8, r: 5.5 },
    waves: { paths: ["M85 -2 Q78 8 85 18", "M115 -2 Q122 8 115 18"], width: 5 },
  },
};

/** Below 32 px: heavier strokes so the face still reads (tab, mobile header). */
export const SMALL: LogoWeight = {
  house: 14.5,
  eye: 11.5,
  smile: 12.5,
  leftEye: "M74 95 Q81 83 88 95",
  rightEye: "M112 95 Q119 83 126 95",
  smilePath: "M68 115 Q100 147 132 115",
  wink: { origin: "119 89", path: "M-7 6 Q0 -6 7 6" },
  antenna: {
    mast: { path: "M100 30 L100 10", width: 11 },
    ball: { cx: 100, cy: 3, r: 8 },
    waves: { paths: ["M80 -5 Q69 4 80 13", "M120 -5 Q131 4 120 13"], width: 8.5 },
  },
};

export const SMALL_BELOW_PX = 32;
