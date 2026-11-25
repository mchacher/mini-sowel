import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  aggregate,
  homeRoutes,
  mockApi,
  resetStores,
  storeSession,
  zone,
  type Route,
} from "../../test-utils";
import { useAuth } from "../../store/useAuth";
import { useZoneAggregation } from "../../store/useZoneAggregation";
import { SettingsPage } from "./SettingsPage";
import { App } from "../../App";

const ROOT = "00000000-0000-0000-0000-000000000001";
const ZONES = ["UTC", "Europe/Paris", "America/New_York"];

beforeEach(() => {
  window.location.hash = "";
});

afterEach(async () => {
  cleanup();
  resetStores();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await new Promise((resolve) => setTimeout(resolve, 120));
});

async function setup(settings: Record<string, string> = {}, routes: Record<string, Route> = {}) {
  const api = mockApi({
    "GET /api/v1/settings": { body: settings },
    "GET /api/v1/settings/timezones": { body: ZONES },
    "GET /api/v1/zones": { body: [zone(ROOT, "Home")] },
    "GET /api/v1/zones/aggregation": { body: {} },
    ...routes,
  });
  render(<SettingsPage />);
  await screen.findByLabelText("Latitude");
  return api;
}

const field = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const save = () => screen.queryByRole("button", { name: "Save" });

describe("SettingsPage, Home card", () => {
  it("fills the fields and the time zone list, Save hidden", async () => {
    await setup({
      "home.name": "Annex",
      "home.latitude": "48.8566",
      "home.longitude": "2.3522",
      "home.timezone": "Europe/Paris",
      "home.sunriseOffset": "10",
    });
    expect(field("Home name").value).toBe("Annex");
    expect(field("Latitude").value).toBe("48.8566");
    expect(field("Longitude").value).toBe("2.3522");
    expect(field("Sunrise offset (min)").value).toBe("10");
    expect(field("Sunset offset (min)").value).toBe("45");
    const select = screen.getByLabelText("Timezone") as HTMLSelectElement;
    expect(select.value).toBe("Europe/Paris");
    expect([...select.options].map((o) => o.value)).toEqual(ZONES);
    expect(save()).toBeNull();
  });

  it("shows UTC when the time zone is unset, and saves the six keys with a comma turned to a dot", async () => {
    const api = await setup({}, { "PUT /api/v1/settings": { body: { success: true } } });
    expect((screen.getByLabelText("Timezone") as HTMLSelectElement).value).toBe("UTC");
    fireEvent.input(field("Latitude"), { target: { value: "48,8566" } });
    expect(field("Latitude").value).toBe("48.8566");
    fireEvent.change(screen.getByLabelText("Timezone"), { target: { value: "Europe/Paris" } });
    fireEvent.click(save()!);
    await waitFor(() => expect(save()).toBeNull());
    expect(api.calls.find((c) => c.key === "PUT /api/v1/settings")!.body).toEqual({
      "home.name": "",
      "home.latitude": "48.8566",
      "home.longitude": "",
      "home.sunriseOffset": "30",
      "home.sunsetOffset": "45",
      "home.timezone": "Europe/Paris",
    });
  });

  it("shows the host's text when the PUT is refused, Save stays", async () => {
    await setup(
      {},
      {
        "PUT /api/v1/settings": { status: 400, body: { error: "Invalid value for home.latitude" } },
      },
    );
    fireEvent.input(field("Latitude"), { target: { value: "99x" } });
    fireEvent.click(save()!);
    expect(await screen.findByText("Invalid value for home.latitude")).toBeTruthy();
    expect(save()).toBeTruthy();
  });

  it("shows today's sun times and Day from the root aggregation, the hint when there are none", async () => {
    await setup();
    expect(screen.getByText("Set the location to get the sun times")).toBeTruthy();
    act(() => {
      useZoneAggregation.setState({
        data: { [ROOT]: aggregate({ sunrise: "05:46", sunset: "21:57", isDaylight: true }) },
      });
    });
    expect(screen.getByText("Sunrise 05:46 · Sunset 21:57 · Day")).toBeTruthy();
    act(() => {
      useZoneAggregation.setState({
        data: { [ROOT]: aggregate({ sunrise: "--:--", sunset: "--:--", isDaylight: false }) },
      });
    });
    expect(screen.getByText("Sunrise --:-- · Sunset --:-- · Night")).toBeTruthy();
  });

  it("speaks French in a French browser", async () => {
    vi.spyOn(navigator, "language", "get").mockReturnValue("fr-FR");
    const { setLanguage, detectLanguage } = await import("../../i18n");
    setLanguage(detectLanguage());
    try {
      mockApi({
        "GET /api/v1/settings": { body: {} },
        "GET /api/v1/settings/timezones": { body: ZONES },
      });
      render(<SettingsPage />);
      expect(await screen.findByLabelText("Fuseau horaire")).toBeTruthy();
      expect(screen.getByText("Aujourd'hui")).toBeTruthy();
      expect(screen.getByRole("button", { name: "Exporter" })).toBeTruthy();
    } finally {
      setLanguage("en");
    }
  });
});

const BACKUP = { version: 1, settings: { "home.name": "Annex" }, zones: [] };

function fileOf(text: string, size = text.length) {
  const file = new File([text], "backup.json", { type: "application/json" });
  Object.defineProperty(file, "size", { value: size });
  return file;
}
const pickFile = (file: File) =>
  fireEvent.change(screen.getByLabelText("Import", { selector: "input" }), {
    target: { files: [file] },
  });

describe("SettingsPage, Backup card", () => {
  it("exports: GET /backup, then a download named by the date", async () => {
    const api = await setup({}, { "GET /api/v1/backup": { body: BACKUP } });
    const create = vi.fn(() => "blob:x");
    vi.stubGlobal("URL", { createObjectURL: create, revokeObjectURL: vi.fn() });
    const names: string[] = [];
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      names.push(this.download);
    });
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect(api.count("GET /api/v1/backup")).toBe(1);
    expect(create).toHaveBeenCalledTimes(1);
    expect(names[0]).toMatch(/^solite-backup-\d{4}-\d{2}-\d{2}\.json$/);
  });

  it("shows the host's text when the export is refused", async () => {
    await setup(
      {},
      { "GET /api/v1/backup": { status: 413, body: { error: "Backup too large: 20000 bytes" } } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    expect(await screen.findByText("Backup too large: 20000 bytes")).toBeTruthy();
  });

  it("refuses a file over 16384 bytes in the page, sends one of exactly 16384", async () => {
    const api = await setup({}, { "POST /api/v1/backup": { body: importResult() } });
    pickFile(fileOf(JSON.stringify(BACKUP), 16385));
    expect(await screen.findByText("File too large (16 KB at most)")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.count("POST /api/v1/backup")).toBe(0);
    pickFile(fileOf(JSON.stringify(BACKUP), 16384));
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(screen.queryByText("File too large (16 KB at most)")).toBeNull();
  });

  it("imports after the confirmation: POST with the file's text, the counts, the stores read again", async () => {
    const api = await setup({}, { "POST /api/v1/backup": { body: importResult() } });
    pickFile(fileOf(JSON.stringify(BACKUP)));
    const dialog = await screen.findByRole("dialog");
    expect(api.count("POST /api/v1/backup")).toBe(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Import" }));
    expect(
      await screen.findByText("Imported: 1 settings, 2 rooms created, 3 rooms updated"),
    ).toBeTruthy();
    expect(api.calls.find((c) => c.key === "POST /api/v1/backup")!.body).toEqual(BACKUP);
    await waitFor(() => expect(api.count("GET /api/v1/zones")).toBe(1));
    await waitFor(() => expect(api.count("GET /api/v1/settings")).toBe(2));
  });

  it("sends nothing when the confirmation is cancelled", async () => {
    const api = await setup({}, { "POST /api/v1/backup": { body: importResult() } });
    pickFile(fileOf(JSON.stringify(BACKUP)));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.count("POST /api/v1/backup")).toBe(0);
  });

  it("shows the host's refusal text when the import is refused", async () => {
    await setup(
      {},
      {
        "POST /api/v1/backup": {
          status: 400,
          body: { error: "Invalid backup: unsupported version 2" },
        },
      },
    );
    pickFile(fileOf(JSON.stringify(BACKUP)));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Import" }));
    expect(await screen.findByText("Invalid backup: unsupported version 2")).toBeTruthy();
  });
});

function importResult() {
  return {
    success: true,
    settings: 1,
    zonesCreated: 2,
    zonesUpdated: 3,
    zonesUnchanged: 0,
  };
}

describe("Settings in the navigation", () => {
  it("has its entry in Administration, and #/settings shows the page", async () => {
    mockApi({
      ...homeRoutes({ zones: [zone(ROOT, "Home")] }),
      "GET /api/v1/settings": { body: {} },
      "GET /api/v1/settings/timezones": { body: ZONES },
    });
    window.location.hash = "#/settings";
    storeSession();
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeTruthy();
    const link = screen.getAllByRole("link", { name: "Settings" })[0];
    expect(link.getAttribute("href")).toBe("#/settings");
    expect(link.getAttribute("aria-current")).toBeTruthy();
  });
});

describe("SettingsPage, factory reset card", () => {
  const open = () => fireEvent.click(screen.getByRole("button", { name: "Factory reset" }));
  const RESET = "POST /api/v1/system/factory-reset";

  it("asks for a confirmation first, and Cancel sends nothing", async () => {
    const api = await setup();
    open();
    const dialog = screen.getByRole("dialog", { name: "Erase everything?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.count(RESET)).toBe(0);
  });

  it("success: sends {} as JSON, clears the tokens and shows the first-start state", async () => {
    storeSession();
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    const api = await setup({}, { [RESET]: { status: 204 } });
    open();
    fireEvent.click(screen.getByRole("button", { name: "Erase everything" }));
    await waitFor(() => expect(useAuth.getState().setupRequired).toBe(true));
    expect(api.calls.find((c) => c.key === RESET)!.body).toEqual({});
    expect(useAuth.getState().isAuthenticated).toBe(false);
    expect(localStorage.getItem("sowel_refresh_token")).toBeNull();
  });

  it("refusal: shows the host's text and keeps the session", async () => {
    storeSession();
    useAuth.setState({ isAuthenticated: true, loading: false, setupRequired: false });
    await setup(
      {},
      {
        [RESET]: {
          status: 403,
          body: { error: "Press the button on the box, then try again", confirmRequired: true },
        },
      },
    );
    open();
    fireEvent.click(screen.getByRole("button", { name: "Erase everything" }));
    expect(await screen.findByText("Press the button on the box, then try again")).toBeTruthy();
    expect(useAuth.getState().isAuthenticated).toBe(true);
    expect(localStorage.getItem("sowel_refresh_token")).toBe("refresh-0");
  });
});
