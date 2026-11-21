import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme } from "./theme";

type Listener = () => void;

/** A `matchMedia` for `prefers-color-scheme: dark` that the test can flip. */
function fakeMatchMedia(dark: boolean) {
  const state = { dark, listeners: new Set<Listener>() };
  vi.stubGlobal("matchMedia", () => ({
    get matches() {
      return state.dark;
    },
    addEventListener: (_: string, l: Listener) => state.listeners.add(l),
    removeEventListener: (_: string, l: Listener) => state.listeners.delete(l),
  }));
  return {
    set(next: boolean) {
      state.dark = next;
      state.listeners.forEach((l) => l());
    },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  document.documentElement.dataset.theme = "hybrid";
});

describe("theme", () => {
  it("follows prefers-color-scheme: dark, then light again", () => {
    const system = fakeMatchMedia(true);
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
    system.set(false);
    expect(document.documentElement.dataset.theme).toBe("hybrid");
    system.set(true);
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
