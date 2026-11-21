// Where a function of the library comes from: shared code, a brand file, a device file, or nowhere.
// The lookup works on plain data (a list of {file, text}) so it is tested without the library.

export type Origin = "shared" | "brand" | "device" | "unknown";

export interface SourceFile {
  /** Path relative to the library's `dist/` folder, with forward slashes: `lib/ikea.js`. */
  file: string;
  text: string;
}

/** The category of one source file: `devices/*` is device, a listed `lib/` file is brand. */
export function fileCategory(
  file: string,
  brandFiles: readonly string[],
): Exclude<Origin, "unknown"> {
  if (file.startsWith("devices/")) return "device";
  if (brandFiles.includes(file)) return "brand";
  return "shared";
}

/**
 * Builds the lookup from a function's source text to its origin. The text is searched in every
 * file; found in one category it gives that category (the same text in two device files stays
 * device), found in two categories or nowhere it is `unknown`.
 */
export function createOriginLookup(
  files: readonly SourceFile[],
  brandFiles: readonly string[],
): (text: string) => Origin {
  const categorised = files.map((f) => ({
    category: fileCategory(f.file, brandFiles),
    text: f.text,
  }));
  const cache = new Map<string, Origin>();
  return (text) => {
    const cached = cache.get(text);
    if (cached) return cached;
    const found = new Set<Origin>();
    if (text.length > 0) {
      for (const f of categorised) {
        if (f.text.includes(text)) found.add(f.category);
      }
    }
    // The widest reuse wins: shared code, then a brand file, then device files.
    const origin: Origin = found.has("shared")
      ? "shared"
      : found.has("brand")
        ? "brand"
        : found.has("device")
          ? "device"
          : "unknown";
    cache.set(text, origin);
    return origin;
  };
}
