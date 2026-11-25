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
const VERSION = { current: "0.1.0", latest: null, updateAvailable: false, releaseUrl: null };
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
    "GET /api/v1/system/version": { body: VERSION },
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

  it("reads the home settings and the time zone again after a successful save, for the top bar", async () => {
    const api = await setup(
      {},
      {
        "PUT /api/v1/settings": { body: { success: true } },
        "GET /api/v1/system/timezone": {
          body: { tz: "Europe/Paris", source: "env", offsetHours: 1 },
        },
      },
    );
    expect(api.count("GET /api/v1/system/timezone")).toBe(0);
    const settingsReads = api.count("GET /api/v1/settings"); // the card's own read
    fireEvent.input(field("Home name"), { target: { value: "Annex" } });
    fireEvent.click(save()!);
    await waitFor(() => expect(save()).toBeNull());
    await waitFor(() => expect(api.count("GET /api/v1/system/timezone")).toBe(1));
    expect(api.count("GET /api/v1/settings")).toBe(settingsReads + 1);
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

const GET = "GET /api/v1/system/backup";
const POST = "POST /api/v1/system/backup";
const PASSWORD = "correct horse";

const exportButton = () => screen.getByRole("button", { name: /^Export/ }) as HTMLButtonElement;
const restoreButton = () => screen.getByRole("button", { name: /^Restor/ }) as HTMLButtonElement;
const typeIn = (label: string, value: string) =>
  fireEvent.input(screen.getByLabelText(label), { target: { value } });
const fillExport = (password = PASSWORD, confirm = PASSWORD) => {
  typeIn("Backup password", password);
  typeIn("Confirm the backup password", confirm);
};
const slb = () => new File(["SOLITEBK"], "box.slb");
const chooseBackup = (file = slb(), password = PASSWORD) => {
  fireEvent.change(screen.getByLabelText("Backup file (.slb)"), { target: { files: [file] } });
  typeIn("Password of the backup", password);
};

describe("SettingsPage, Backup card", () => {
  it("keeps Export disabled until the two passwords are equal and 8 to 128 characters, with the reason", async () => {
    await setup({}, { [GET]: { body: {} } });
    expect(exportButton().disabled).toBe(true);
    fillExport("short", "short");
    expect(exportButton().disabled).toBe(true);
    expect(screen.getByText("Password must be at least 8 characters")).toBeTruthy();
    fillExport(PASSWORD, "correct horsE");
    expect(exportButton().disabled).toBe(true);
    expect(screen.getByText("Passwords do not match")).toBeTruthy();
    fillExport("x".repeat(129), "x".repeat(129));
    expect(exportButton().disabled).toBe(true);
    expect(screen.getByText("Password must be at most 128 characters")).toBeTruthy();
    fillExport("x".repeat(128), "x".repeat(128));
    expect(exportButton().disabled).toBe(false);
  });

  it("exports: GET with the token and the percent-encoded password header, then a .slb download named by the date", async () => {
    storeSession();
    const api = await setup({}, { [GET]: { body: {} } });
    const created: Blob[] = [];
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn((b: Blob) => (created.push(b), "blob:x")),
      revokeObjectURL: vi.fn(),
    });
    const names: string[] = [];
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      names.push(this.download);
    });
    const password = "mot de passe é 😀";
    fillExport(password, password);
    fireEvent.click(exportButton());
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect(api.count(GET)).toBe(1);
    const at = api.calls.findIndex((c) => c.key === GET);
    expect(api.headersOf[at]!["Authorization"]).toMatch(/^Bearer /);
    expect(api.headersOf[at]!["X-Backup-Password"]).toBe(encodeURIComponent(password));
    expect(created).toHaveLength(1);
    expect(created[0]).toBeInstanceOf(Blob);
    expect(names[0]).toMatch(/^solite-backup-\d{4}-\d{2}-\d{2}\.slb$/);
  });

  it("shows the host's text when the export is refused", async () => {
    await setup({}, { [GET]: { status: 409, body: { error: "Backup already in progress" } } });
    fillExport();
    fireEvent.click(exportButton());
    expect(await screen.findByText("Backup already in progress")).toBeTruthy();
  });

  it("shows the server's text for a 503 on the export and a 500 on the restore", async () => {
    await setup(
      {},
      {
        [GET]: { status: 503, body: { error: "Busy saving, try again" } },
        [POST]: { status: 500, body: { error: "Restore failed: no space left" } },
      },
    );
    fillExport();
    fireEvent.click(exportButton());
    // A GET answered 503 is retried once after 1 s by the client.
    expect(await screen.findByText("Busy saving, try again", {}, { timeout: 4000 })).toBeTruthy();
    chooseBackup();
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restore" }),
    );
    expect(await screen.findByText("Restore failed: no space left")).toBeTruthy();
  });

  it("keeps Restore disabled until a file and a password are given", async () => {
    await setup();
    expect(restoreButton().disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Backup file (.slb)"), { target: { files: [slb()] } });
    await waitFor(() => expect(restoreButton().disabled).toBe(true));
    typeIn("Password of the backup", PASSWORD);
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
  });

  it("restores after the confirmation: POST of the file as octet-stream with the token and the password, the message shown", async () => {
    storeSession();
    const api = await setup(
      {},
      {
        [POST]: {
          body: {
            success: true,
            message: "Backup restored: the box restarts, log in again after it",
            files: 5,
            wifi: false,
          },
        },
      },
    );
    const file = slb();
    chooseBackup(file);
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/The account is kept/)).toBeTruthy();
    expect(api.count(POST)).toBe(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    expect(
      await screen.findByText("Backup restored: the box restarts, log in again after it"),
    ).toBeTruthy();
    const at = api.calls.findIndex((c) => c.key === POST);
    expect(api.headersOf[at]!["Content-Type"]).toBe("application/octet-stream");
    expect(api.headersOf[at]!["Authorization"]).toMatch(/^Bearer /);
    expect(api.headersOf[at]!["X-Backup-Password"]).toBe(encodeURIComponent(PASSWORD));
    expect(api.calls[at]!.body).toBe(file);
  });

  it("asks for the Wi-Fi settings only when the box is ticked: ?wifi=true", async () => {
    const api = await setup(
      {},
      {
        "POST /api/v1/system/backup?wifi=true": {
          body: { success: true, message: "Backup restored", files: 6, wifi: true },
        },
      },
    );
    chooseBackup();
    fireEvent.click(screen.getByLabelText("Also restore the Wi-Fi settings"));
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("The Wi-Fi settings are replaced too.")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    expect(await screen.findByText("Backup restored")).toBeTruthy();
    expect(api.count("POST /api/v1/system/backup?wifi=true")).toBe(1);
    expect(api.count(POST)).toBe(0);
  });

  it("sends nothing when the confirmation is cancelled", async () => {
    const api = await setup({}, { [POST]: { body: {} } });
    chooseBackup();
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.count(POST)).toBe(0);
  });

  it("shows Restoring... and disables the button while the upload runs", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    await setup(
      {},
      {
        [POST]: async () => {
          await gate;
          return { body: { success: true, message: "Backup restored", files: 1, wifi: false } };
        },
      },
    );
    chooseBackup();
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restore" }),
    );
    const busy = await screen.findByRole("button", { name: "Restoring..." });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    release();
    expect(await screen.findByText("Backup restored")).toBeTruthy();
  });

  it("shows the host's text when the restore is refused (wrong password)", async () => {
    await setup({}, { [POST]: { status: 400, body: { error: "Wrong backup password" } } });
    chooseBackup();
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restore" }),
    );
    expect(await screen.findByText("Wrong backup password")).toBeTruthy();
  });

  it("shows the hint of a confirmRequired refusal", async () => {
    await setup(
      {},
      {
        [POST]: {
          status: 403,
          body: { error: "Press the button on the box, then try again", confirmRequired: true },
        },
      },
    );
    chooseBackup();
    await waitFor(() => expect(restoreButton().disabled).toBe(false));
    fireEvent.click(restoreButton());
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restore" }),
    );
    expect(await screen.findByText("Press the button on the box, then try again")).toBeTruthy();
  });
});

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

describe("SettingsPage, Update card", () => {
  const sup = () => new File(["SUP"], "solite.sup");
  const choose = (file: File) =>
    fireEvent.change(screen.getByLabelText("Update file"), { target: { files: [file] } });
  const install = () => screen.getByRole("button", { name: /^Install/ }) as HTMLButtonElement;

  it("shows the running version", async () => {
    await setup();
    expect(await screen.findByText("Running version: 0.1.0")).toBeTruthy();
  });

  it("keeps Install disabled until a file is chosen", async () => {
    await setup();
    expect(install().disabled).toBe(true);
    choose(sup());
    await waitFor(() => expect(install().disabled).toBe(false));
  });

  it("posts the file as octet-stream with the token, shows the message and the reload line", async () => {
    storeSession();
    const api = await setup(
      {},
      { "POST /api/v1/system/update": { body: { message: "Installed 0.2.0, restarting" } } },
    );
    choose(sup());
    fireEvent.click(install());
    expect(await screen.findByText("Installed 0.2.0, restarting")).toBeTruthy();
    expect(screen.getByText("The box restarts; reload the page in a minute.")).toBeTruthy();
    const at = api.calls.findIndex((c) => c.key === "POST /api/v1/system/update");
    const call = api.calls[at]!;
    expect(api.headersOf[at]!["Content-Type"]).toBe("application/octet-stream");
    expect(api.headersOf[at]!["Authorization"]).toMatch(/^Bearer /);
    expect(call.body).toBeInstanceOf(File);
  });

  it("shows the error text of a refused file, no reload line", async () => {
    await setup(
      {},
      { "POST /api/v1/system/update": { status: 400, body: { error: "Bad signature" } } },
    );
    choose(sup());
    fireEvent.click(install());
    expect(await screen.findByText("Bad signature")).toBeTruthy();
    expect(screen.queryByText("The box restarts; reload the page in a minute.")).toBeNull();
  });

  it("shows Installing… and disables the button while the upload runs", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    await setup(
      {},
      {
        "POST /api/v1/system/update": async () => {
          await gate;
          return { body: { message: "Installed 0.2.0, restarting" } };
        },
      },
    );
    choose(sup());
    fireEvent.click(install());
    const busy = await screen.findByRole("button", { name: "Installing…" });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    release();
    expect(await screen.findByText("Installed 0.2.0, restarting")).toBeTruthy();
  });
});
