import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecipeStore, Resources, StoreEntry } from "../../api";
import { mockApi, resetStores } from "../../test-utils";
import { RecipesCard } from "./RecipesCard";

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

const resources = (refused: string[] = []): Resources => ({
  internal: null,
  psram: null,
  storage: { used: 0, total: null },
  scripts: refused.map((id) => ({
    id,
    version: null,
    source: "installed",
    state: "refused",
    reason: "Invalid signature",
    held: 0,
    limit: 0,
    instances: 0,
    calls: 0,
    averageUs: 0,
    maxUs: 0,
  })),
});

describe("RecipesCard", () => {
  it("lists the catalogue, reading it again when it opens", async () => {
    const held = store([entry("a", "1.0.0")]);
    const fresh = store([entry("a", "1.0.0"), entry("b", "2.0.0")]);
    const api = mockApi({
      [STORE]: { body: held },
      [REFRESH]: { body: fresh },
      [RESOURCES]: { body: resources() },
    });
    render(<RecipesCard />);
    expect(await screen.findByTestId("recipe-b")).toBeTruthy();
    expect(api.count(REFRESH)).toBe(1);
    const a = screen.getByTestId("recipe-a");
    expect(a.textContent).toContain("Recipe a");
    expect(a.textContent).toContain("What a does");
    expect(a.textContent).toContain("Not installed");
    expect(within(a).getByRole("button", { name: "Install" })).toBeTruthy();
    expect(within(a).queryByRole("button", { name: "Remove" })).toBeNull();
  });

  it("shows the list held when the refresh answers 429", async () => {
    mockApi({
      [STORE]: { body: store([entry("a", "1.0.0")]) },
      [REFRESH]: { status: 429, body: { error: "Please wait 42s before refreshing again" } },
      [RESOURCES]: { body: resources() },
    });
    render(<RecipesCard />);
    expect(await screen.findByTestId("recipe-a")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("recipe-a")).toBeTruthy());
    expect(screen.queryByText(/Please wait/)).toBeNull();
  });

  it("installs a recipe and shows the answer's list", async () => {
    const after = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    const api = mockApi({
      [STORE]: [{ body: store([entry("a", "1.0.0")]) }, { body: after }],
      [REFRESH]: { body: store([entry("a", "1.0.0")]) },
      [INSTALL]: { status: 201, body: { id: "a", version: "1.0.0" } },
      [RESOURCES]: { body: resources() },
    });
    render(<RecipesCard />);
    const a = await screen.findByTestId("recipe-a");
    fireEvent.click(within(a).getByRole("button", { name: "Install" }));
    await screen.findByText("Recipe installed.");
    expect(api.calls.find((c) => c.key === INSTALL)?.body).toEqual({ id: "a" });
    await waitFor(() =>
      expect(screen.getByTestId("recipe-a").textContent).toContain("Installed: 1.0.0"),
    );
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
    render(<RecipesCard />);
    const a = await screen.findByTestId("recipe-a");
    expect(within(a).queryByRole("button", { name: "Install" })).toBeNull();
    fireEvent.click(within(a).getByRole("button", { name: "Update" }));
    await screen.findByText("Recipe updated.");
    expect(api.count(UPDATE)).toBe(1);
    fireEvent.click(within(screen.getByTestId("recipe-a")).getByRole("button", { name: "Remove" }));
    await screen.findByText("Recipe removed.");
    expect(api.count(UNINSTALL_A)).toBe(1);
  });

  it("shows no Update when the installed version is the catalogue's", async () => {
    const same = store([entry("a", "1.0.0", { installed: "1.0.0" })]);
    mockApi({
      [STORE]: { body: same },
      [REFRESH]: { body: same },
      [RESOURCES]: { body: resources() },
    });
    render(<RecipesCard />);
    const a = await screen.findByTestId("recipe-a");
    expect(within(a).queryByRole("button", { name: "Update" })).toBeNull();
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
    render(<RecipesCard />);
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
    render(<RecipesCard />);
    await screen.findByText("The catalogue lists no recipe.");
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

  it("lists a refused file with its reason and removes it", async () => {
    const api = mockApi({
      [STORE]: { body: store([]) },
      [REFRESH]: { body: store([]) },
      [RESOURCES]: [{ body: resources(["bad"]) }, { body: resources() }],
      [UNINSTALL_BAD]: { status: 204 },
    });
    render(<RecipesCard />);
    const bad = await screen.findByTestId("refused-bad");
    expect(bad.textContent).toContain("File refused: Invalid signature");
    fireEvent.click(within(bad).getByRole("button", { name: "Remove" }));
    await screen.findByText("Recipe removed.");
    expect(api.count(UNINSTALL_BAD)).toBe(1);
    await waitFor(() => expect(screen.queryByTestId("refused-bad")).toBeNull());
  });

  it("shows the error of the last read and keeps the list", async () => {
    mockApi({
      [STORE]: { body: store([entry("a", "1.0.0")], "no route to host") },
      [REFRESH]: { body: store([entry("a", "1.0.0")], "no route to host") },
      [RESOURCES]: { body: resources() },
    });
    render(<RecipesCard />);
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
    render(<RecipesCard />);
    const row = await screen.findByTestId("recipe-motion-light");
    expect(row.textContent).toContain("Provided by Solite");
    expect(within(row).queryByRole("button")).toBeNull();
  });
});
