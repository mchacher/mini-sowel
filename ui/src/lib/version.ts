// Spec 040, R4.6: "newer" is a numeric comparison of the three numbers (0.10.0 is newer than 0.9.0).

const parts = (v: string): number[] =>
  v
    .replace(/^v/, "")
    .split("-")[0]!
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);

/** Negative when `a` is older than `b`, 0 when equal, positive when newer. */
export function compareVersions(a: string, b: string): number {
  const x = parts(a);
  const y = parts(b);
  for (let i = 0; i < Math.max(x.length, y.length, 3); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
