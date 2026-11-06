// Shared motion: durations (ms) match --t-fast / --t / --t-slow in styles/base.css.
// `ease` is the sync bar's curve (.4, 0, .2, 1); `easeOut` (.2, .8, .2, 1) starts fast for entrances.
function bezier(x1, y1, x2, y2) {
  const axis = (a, b, t) => 3 * a * (1 - t) ** 2 * t + 3 * b * (1 - t) * t ** 2 + t ** 3;
  return (x) => {
    if (x <= 0 || x >= 1) return x;
    let t = x;
    for (let i = 0; i < 8; i++) {
      const err = axis(x1, x2, t) - x;
      if (Math.abs(err) < 1e-5) break;
      const slope = 3 * x1 * (1 - t) ** 2 + 6 * (x2 - x1) * (1 - t) * t + 3 * (1 - x2) * t ** 2;
      if (!slope) break;
      t -= err / slope;
    }
    return axis(y1, y2, t);
  };
}

export const ease = bezier(0.4, 0, 0.2, 1);
export const easeOut = bezier(0.2, 0.8, 0.2, 1);
export const DUR = { fast: 120, base: 180, slow: 240 };
