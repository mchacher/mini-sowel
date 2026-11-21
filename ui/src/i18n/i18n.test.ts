import { afterEach, describe, expect, it, vi } from "vitest";
import i18n, { detectLanguage, setLanguage, t } from "./index";

afterEach(() => {
  vi.restoreAllMocks();
  setLanguage("en");
});

describe("t()", () => {
  it("gives a key in English and in French, and an unknown key as itself", () => {
    setLanguage("en");
    expect(t("controls.turnOn")).toBe("Turn on");
    setLanguage("fr");
    expect(t("controls.turnOn")).toBe("Allumer");
    expect(t("no.such.key")).toBe("no.such.key");
  });

  it("chooses the plural form by the count, in each language", () => {
    setLanguage("en");
    expect(t("zone.lead.lightsOn", { count: 1 })).toBe("1 light on");
    expect(t("zone.lead.lightsOn", { count: 3 })).toBe("3 lights on");
    setLanguage("fr");
    expect(t("zone.lead.lightsOn", { count: 1 })).toBe("1 lumière allumée");
    expect(t("zone.lead.lightsOn", { count: 3 })).toBe("3 lumières allumées");
  });

  it("fills {{name}}, and falls back to defaultValue (also interpolated) then to the key", () => {
    expect(t("zones.aggregate.offline", { names: "Garage light" })).toBe("Offline: Garage light");
    expect(t("unknown.key", { defaultValue: "{{count}} thing", count: 2 })).toBe("2 thing");
    expect(t("unknown.key", { defaultValue: "fallback" })).toBe("fallback");
  });

  it("is French for a French browser, and <html lang> follows", () => {
    vi.spyOn(navigator, "language", "get").mockReturnValue("fr-FR");
    expect(detectLanguage()).toBe("fr");
    setLanguage(detectLanguage());
    expect(i18n.language).toBe("fr");
    expect(document.documentElement.lang).toBe("fr");
    expect(t("app.noRoom")).toBe("Aucune pièce : créez-en une avec l'API");
    vi.spyOn(navigator, "language", "get").mockReturnValue("de-DE");
    expect(detectLanguage()).toBe("en");
  });
});
