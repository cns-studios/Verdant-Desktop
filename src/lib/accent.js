// Derives the full accent token set from a single seed colour, in OKLCH so lightness
// steps look even across hues. Mirrors the token set of the preset schemes in base.css.

export const DEFAULT_CUSTOM_COLOR = "#4a7fb5";
export const ACCENT_VARS = [
  "--accent",
  "--accent-mid",
  "--accent-light",
  "--accent-muted",
  "--accent-pale",
  "--accent-color",
  "--on-accent",
  "--accent-lightmode",
  "--accent-darkmode",
];

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

export function normalizeHex(raw) {
  let s = String(raw ?? "").trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(s)) s = [...s].map((c) => c + c).join("");
  return /^[0-9a-f]{6}$/.test(s) ? `#${s}` : null;
}

const toLinear = (v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const fromLinear = (v) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);

function hexToLinear(hex) {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => toLinear(v / 255));
}

export function hexToOklch(hex) {
  const [r, g, b] = hexToLinear(hex);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { l: L, c: Math.hypot(a, bb), h: ((Math.atan2(bb, a) * 180) / Math.PI + 360) % 360 };
}

function oklchToLinear(L, c, h) {
  const a = c * Math.cos((h * Math.PI) / 180);
  const b = c * Math.sin((h * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const inGamut = (rgb) => rgb.every((v) => v >= -0.0005 && v <= 1.0005);

/** OKLCH → hex, reducing chroma until the colour fits sRGB. */
export function oklchToHex(L, c, h) {
  let lo = 0;
  let hi = Math.max(0, c);
  let rgb = oklchToLinear(L, hi, h);
  if (!inGamut(rgb)) {
    for (let i = 0; i < 18; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToLinear(L, mid, h))) lo = mid;
      else hi = mid;
    }
    rgb = oklchToLinear(L, lo, h);
  }
  return (
    "#" +
    rgb
      .map((v) => Math.round(clamp(fromLinear(clamp(v, 0, 1)), 0, 1) * 255).toString(16).padStart(2, "0"))
      .join("")
  );
}

function luminance(hex) {
  const [r, g, b] = hexToLinear(hex);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Text colour for content on top of the accent. */
function onAccent(accent) {
  return contrast(accent, "#ffffff") >= contrast(accent, "#1a1a1a") ? "#ffffff" : "#1a1a1a";
}

/**
 * Derive the token set for one mode from a seed hex.
 * The seed hue/chroma are kept; lightness is clamped so the accent stays legible on the mode's surfaces.
 */
export function deriveAccent(hex, dark) {
  const seed = normalizeHex(hex) ?? DEFAULT_CUSTOM_COLOR;
  const { l, c, h } = hexToOklch(seed);
  const hue = h;
  const chroma = clamp(c, 0.02, 0.2); // near-greys still get a hint of tint

  const L = dark ? clamp(l, 0.52, 0.72) : clamp(l, 0.4, 0.58);
  const accent = oklchToHex(L, chroma, hue);

  const tokens = dark
    ? {
        "--accent": accent,
        "--accent-mid": oklchToHex(L - 0.04, chroma * 0.9, hue),
        "--accent-light": oklchToHex(L + 0.06, chroma * 0.85, hue),
        "--accent-muted": oklchToHex(0.31, chroma * 0.4, hue),
        "--accent-pale": oklchToHex(0.29, chroma * 0.35, hue),
        "--accent-color": oklchToHex(clamp(L + 0.08, 0, 0.82), chroma, hue),
      }
    : {
        "--accent": accent,
        "--accent-mid": oklchToHex(L + 0.06, chroma * 0.95, hue),
        "--accent-light": oklchToHex(L + 0.17, chroma * 0.85, hue),
        "--accent-muted": oklchToHex(0.88, chroma * 0.3, hue),
        "--accent-pale": oklchToHex(0.955, chroma * 0.14, hue),
        "--accent-color": oklchToHex(L - 0.07, chroma, hue),
      };

  tokens["--on-accent"] = onAccent(accent);
  tokens["--accent-lightmode"] = oklchToHex(0.88, chroma * 0.3, hue);
  tokens["--accent-darkmode"] = oklchToHex(0.31, chroma * 0.4, hue);
  return tokens;
}

/** Both modes at once; this is what gets cached for the pre-paint boot script. */
export function deriveAccentSet(hex) {
  return { light: deriveAccent(hex, false), dark: deriveAccent(hex, true) };
}
