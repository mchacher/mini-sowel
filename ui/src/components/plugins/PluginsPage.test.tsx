import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecipeStore, Resources, StoreEntry } from "../../api";
import { mockApi, resetStores } from "../../test-utils";
import { resetRecipeUpdates } from "../layout/useRecipeUpdates";
import { Sidebar } from "../layout/Sidebar";
import { PluginsPage } from "./PluginsPage";

const STORE = "GET /api/v1/recipes/store";
const REFRESH = "POST /api/v1/recipes/store/refresh";
const INSTALL = "POST /api/v1/recipes/install";
const UPDATE = "POST /api/v1/recipes/a/update";
const UNINSTALL_A = "POST /api/v1/recipes/a/uninstall";
const UNINSTALL_BAD = "POST /api/v1/recipes/bad/uninstall";
const RESOURCES = "GET /api/v1/system/resources";

afterEach(() => {
  cleanup();
  resetStores();
  resetRecipeUpdates();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const entry = (id: string, version: string, extra: Partial<StoreEntry> = {}): StoreEntry => ({
  id,
  version,
  name: `Recipe ${id}`,
  description: `What ${id} does`,
  size: 3000,
  sha256: "0".repeat(64),
  url: `https://example.com/${id}.srs`,
  installed: null,
  refused: null,
  builtIn: false,
  ...extra,
});

const store = (recipes: StoreEntry[], error: string | null = null): RecipeStore => ({
  recipes,
  readAt: "2026-11-27T10:00:00.000Z",
  error,
});

const resources = (refused: string[] = [], uploaded: string[] = []): Resources => ({
  internal: null,
  psram: null,
  storage: { used: 0, total: null },
  scripts: [...refused, ...uploaded].map((id) => ({
    id,
    version: refused.includes(id) ? null : "0.1.0",
    source: "installed",
    state: refused.includes(id) ? "refused" : "ready",
    reason: refused.includes(id) ? "Invalid signature" : null,
    held: 0,
    limit: 0,
    instances: 0,
    calls: 0,
    averageUs: 0,
    maxUs: 0,
  })),
});

/** Remove asks a second click ("Confirm?") before it uninstalls. */
const remove = (row: HTMLElement) => {
  fireEvent.click(within(row).getByRole("button", { name: "Remove" }));
  fireEvent.click(within(row).getByRole("button", { name: "Confirm?" }));
};

const tab = (name: RegExp) => fireEvent.click(screen.getByRole("tab", { name }));

describe("PluginsPage", () => {
  it("lists the installed recipes first, with the counts of both tabs", async () => {
    const both = store([entry("a", "1.0.0", { installed: "1.0.0" }), entry("b", "2.0.0")]);
    mockApi({
      [STORE]: { body: both },
      [REFRESH]: { body: both },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    const a = await screen.findByTestId("recipe-a");
    expect(a.textContent).toContain("Recipe a");
    expect(a.textContent).toContain("1.0.0");
    expect(within(a).getByRole("button", { name: "Remove" })).toBeTruthy();
    expect(within(a).queryByRole("button", { name: "Update" })).toBeNull();
    expect(screen.queryByTestId("recipe-b")).toBeNull();
    expect(screen.getByRole("tab", { name: /^Installed/ }).textContent).toBe("Installed1");
    expect(screen.getByRole("tab", { name: /^Store/ }).textContent).toBe("Store2");
  });

  it("lists the catalogue in the Store tab, reading it again when the page opens", async () => {
    const held = store([entry("a", "1.0.0")]);
    const fresh = store([entry("a", "1.0.0"), entry("b", "2.0.0")]);
    const api = mockApi({
      [STORE]: { body: held },
      [REFRESH]: { body: fresh },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    expect(await screen.findByTestId("recipe-b")).toBeTruthy();
    expect(api.count(REFRESH)).toBe(1);
    const a = screen.getByTestId("recipe-a");
    expect(a.textContent).toContain("What a does");
    expect(within(a).getByRole("button", { name: "Install" })).toBeTruthy();
  });

  it("shows the list held when the refresh answers 429", async () => {
    mockApi({
      [STORE]: { body: store([entry("a", "1.0.0")]) },
      [REFRESH]: { status: 429, body: { error: "Please wait 42s before refreshing again" } },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    expect(await screen.findByTestId("recipe-a")).toBeTruthy();
    expect(screen.queryByText(/Please wait/)).toBeNull();
  });

  it("installs a recipe from the Store tab and shows it installed", async () => {
    const after = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    const api = mockApi({
      [STORE]: [{ body: store([entry("a", "1.0.0")]) }, { body: after }],
      [REFRESH]: { body: store([entry("a", "1.0.0")]) },
      [INSTALL]: { status: 201, body: { id: "a", version: "1.0.0" } },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    fireEvent.click(
      within(await screen.findByTestId("recipe-a")).getByRole("button", { name: "Install" }),
    );
    await screen.findByText("Recipe installed.");
    expect(api.calls.find((c) => c.key === INSTALL)?.body).toEqual({ id: "a" });
    await waitFor(() =>
      expect(
        within(screen.getByTestId("recipe-a")).queryByRole("button", { name: "Install" }),
      ).toBeNull(),
    );
    expect(screen.getByTestId("recipe-a").textContent).toContain("Installed");
    tab(/^Installed/);
    expect(
      within(screen.getByTestId("recipe-a")).getByRole("button", { name: "Remove" }),
    ).toBeTruthy();
  });

  it("offers Update when the catalogue has a newer version, and Remove", async () => {
    const installed = store([entry("a", "1.0.1", { installed: "1.0.0" })]);
    const api = mockApi({
      [STORE]: { body: installed },
      [REFRESH]: { body: installed },
      [UPDATE]: { body: { id: "a", version: "1.0.1" } },
      [UNINSTALL_A]: { status: 204 },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    const a = await screen.findByTestId("recipe-a");
    fireEvent.click(within(a).getByRole("button", { name: "Update" }));
    await screen.findByText("Recipe updated.");
    expect(api.count(UPDATE)).toBe(1);
    remove(screen.getByTestId("recipe-a"));
    await screen.findByText("Recipe removed.");
    expect(api.count(UNINSTALL_A)).toBe(1);
  });

  it("offers Update in the Store tab too, beside the Installed mark", async () => {
    const newer = store([entry("a", "1.0.1", { installed: "1.0.0" })]);
    mockApi({
      [STORE]: { body: newer },
      [REFRESH]: { body: newer },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByTestId("recipe-a");
    tab(/^Store/);
    const a = screen.getByTestId("recipe-a");
    expect(within(a).getByRole("button", { name: "Update" })).toBeTruthy();
    expect(a.textContent).toContain("Installed");
    expect(within(a).queryByRole("button", { name: "Install" })).toBeNull();
  });

  it("shows the server's reason when an install is refused", async () => {
    mockApi({
      [STORE]: { body: store([entry("a", "1.0.0")]) },
      [REFRESH]: { body: store([entry("a", "1.0.0")]) },
      [INSTALL]: {
        status: 409,
        body: { error: "Not enough memory for a: 700 KB free, 736 KB needed" },
      },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    fireEvent.click(
      within(await screen.findByTestId("recipe-a")).getByRole("button", { name: "Install" }),
    );
    expect(
      await screen.findByText("Not enough memory for a: 700 KB free, 736 KB needed"),
    ).toBeTruthy();
  });

  it("uploads a signed file as an octet stream", async () => {
    const api = mockApi({
      [STORE]: { body: store([]) },
      [REFRESH]: { body: store([]) },
      [INSTALL]: { status: 201, body: { id: "a", version: "1.0.0" } },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    const file = new File([new Uint8Array([1, 2, 3])], "a.srs", {
      type: "application/octet-stream",
    });
    fireEvent.change(screen.getByLabelText("Signed recipe file"), { target: { files: [file] } });
    await screen.findByText("Recipe installed from the file.");
    const call = api.calls.findIndex((c) => c.key === INSTALL);
    expect(api.calls[call]?.body).toBe(file);
    expect(api.headersOf[call]?.["Content-Type"]).toBe("application/octet-stream");
    expect(screen.getByRole("button", { name: "Upload a signed recipe" })).toBeTruthy();
  });

  it("lists an uploaded recipe the catalogue lacks, and removes it", async () => {
    const api = mockApi({
      [STORE]: { body: store([]) },
      [REFRESH]: { body: store([]) },
      [RESOURCES]: [{ body: resources([], ["mine"]) }, { body: resources() }],
      "POST /api/v1/recipes/mine/uninstall": { status: 204 },
    });
    render(<PluginsPage />);
    const mine = await screen.findByTestId("refused-mine");
    expect(mine.textContent).toContain("0.1.0");
    remove(mine);
    await screen.findByText("Recipe removed.");
    expect(api.count("POST /api/v1/recipes/mine/uninstall")).toBe(1);
  });

  it("lists a refused file with its reason and removes it", async () => {
    const api = mockApi({
      [STORE]: { body: store([]) },
      [REFRESH]: { body: store([]) },
      [RESOURCES]: [{ body: resources(["bad"]) }, { body: resources() }],
      [UNINSTALL_BAD]: { status: 204 },
    });
    render(<PluginsPage />);
    const bad = await screen.findByTestId("refused-bad");
    expect(bad.textContent).toContain("File refused: Invalid signature");
    remove(bad);
    await screen.findByText("Recipe removed.");
    expect(api.count(UNINSTALL_BAD)).toBe(1);
    await waitFor(() => expect(screen.queryByTestId("refused-bad")).toBeNull());
  });

  it("shows the error of the last read in the Store tab and keeps the list", async () => {
    mockApi({
      [STORE]: { body: store([entry("a", "1.0.0")], "no route to host") },
      [REFRESH]: { body: store([entry("a", "1.0.0")], "no route to host") },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    expect((await screen.findByTestId("read-error")).textContent).toBe(
      "The catalogue could not be read: no route to host",
    );
    expect(screen.getByTestId("recipe-a")).toBeTruthy();
  });

  it("marks a recipe the program provides itself and offers nothing on it", async () => {
    const native = store([entry("motion-light", "1.0.0", { builtIn: true })]);
    mockApi({
      [STORE]: { body: native },
      [REFRESH]: { body: native },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByText("No plugins installed");
    tab(/^Store/);
    const row = await screen.findByTestId("recipe-motion-light");
    expect(row.textContent).toContain("Provided by Solite");
    expect(within(row).queryByRole("button")).toBeNull();
  });

  it("searches the list of the active tab by id, name and description", async () => {
    const both = store([
      entry("a", "1.0.0", { installed: "1.0.0" }),
      entry("b", "1.0.0", { installed: "1.0.0", name: "Night light" }),
    ]);
    mockApi({
      [STORE]: { body: both },
      [REFRESH]: { body: both },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByTestId("recipe-a");
    const field = screen.getByLabelText("Search plugins...");
    fireEvent.input(field, { target: { value: "night" } });
    expect(screen.queryByTestId("recipe-a")).toBeNull();
    expect(screen.getByTestId("recipe-b")).toBeTruthy();
    fireEvent.input(field, { target: { value: "zzz" } });
    expect(screen.getByText('No plugins match "zzz"')).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(screen.getByTestId("recipe-a")).toBeTruthy();
    tab(/^Store/);
    fireEvent.input(field, { target: { value: "what b" } });
    expect(screen.queryByTestId("recipe-a")).toBeNull();
    expect(screen.getByTestId("recipe-b")).toBeTruthy();
  });

  it("uninstalls only on the second click of Remove", async () => {
    const both = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    const api = mockApi({
      [STORE]: { body: both },
      [REFRESH]: { body: both },
      [RESOURCES]: { body: resources() },
      [UNINSTALL_A]: { status: 204 },
    });
    render(<PluginsPage />);
    const a = await screen.findByTestId("recipe-a");
    fireEvent.click(within(a).getByRole("button", { name: "Remove" }));
    expect(within(a).getByRole("button", { name: "Confirm?" })).toBeTruthy();
    expect(api.count(UNINSTALL_A)).toBe(0);
    fireEvent.focusOut(within(a).getByRole("button", { name: "Confirm?" }));
    expect(await within(a).findByRole("button", { name: "Remove" })).toBeTruthy();
    expect(api.count(UNINSTALL_A)).toBe(0);
  });

  it("says why when the refresh at opening is refused, not that the catalogue is empty", async () => {
    const never: RecipeStore = { recipes: [], readAt: null, error: null };
    mockApi({
      [STORE]: { body: never },
      [REFRESH]: { status: 503, body: { error: "Not enough memory for the catalogue" } },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    expect((await screen.findByRole("alert")).textContent).toContain("Not enough memory");
    tab(/^Store/);
    expect(screen.queryByText("The catalogue lists no recipe.")).toBeNull();
  });

  it("keeps the list silently when the refresh is paced (429)", async () => {
    const held = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    mockApi({
      [STORE]: { body: held },
      [REFRESH]: { status: 429, body: { error: "Too soon" } },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByTestId("recipe-a");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("clears the error of the first read when the refresh succeeds", async () => {
    const ok = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    mockApi({
      [STORE]: { status: 500, body: { error: "Held list unreadable" } },
      [REFRESH]: { body: ok },
      [RESOURCES]: { body: resources() },
    });
    render(<PluginsPage />);
    await screen.findByTestId("recipe-a");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("feeds the sidebar's badge: an Update takes the count down", async () => {
    const before = store([entry("a", "2.0.0", { installed: "1.0.0" })]);
    const after = store([entry("a", "2.0.0", { installed: "2.0.0" })]);
    mockApi({
      [STORE]: [{ body: before }, { body: before }, { body: after }],
      [REFRESH]: { body: before },
      [RESOURCES]: { body: resources() },
      [UPDATE]: { status: 204 },
    });
    render(
      <>
        <Sidebar zones={[]} activeId={null} page="plugins" />
        <PluginsPage />
      </>,
    );
    expect((await screen.findByTestId("plugins-badge")).textContent).toBe("1");
    fireEvent.click(await screen.findByRole("button", { name: "Update" }));
    await waitFor(() => expect(screen.queryByTestId("plugins-badge")).toBeNull());
  });
});
