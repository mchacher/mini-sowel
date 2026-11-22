import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { advance, mockApi, resetStores, zone, type Route } from "../../test-utils";
import { useZones } from "../../store/useZones";
import { ZonesPage } from "./ZonesPage";

const tree = () => [
  zone("root", "Home", [zone("rdc", "RDC", [zone("garage", "Garage", [], "rdc")], "root")]),
];

beforeEach(() => {
  window.location.hash = "";
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(async () => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await new Promise((resolve) => setTimeout(resolve, 120));
});

function setup(routes: Record<string, Route | Route[]> = {}, zones = tree()) {
  const api = mockApi({ "GET /api/v1/zones": { body: zones }, ...routes });
  useZones.setState({ tree: zones });
  render(<ZonesPage />);
  return api;
}

const row = (name: string) => screen.getByText(name).closest("a")!.parentElement as HTMLElement;
const buttonOf = (name: string, label: string) =>
  within(row(name)).queryByRole("button", { name: label });

describe("ZonesPage", () => {
  it("shows the tree indented, with the count of zones and of child zones", () => {
    setup();
    expect(screen.getByRole("heading", { level: 1, name: "Home Topology" })).toBeTruthy();
    expect(screen.getByText("3 zones")).toBeTruthy();
    expect(row("Home").style.paddingLeft).toBe("16px");
    expect(row("RDC").style.paddingLeft).toBe("40px");
    expect(row("Garage").style.paddingLeft).toBe("64px");
    expect(within(row("Home")).getByText("1 zone")).toBeTruthy();
    expect(within(row("Garage")).queryByText(/zone/)).toBeNull();
    expect(buttonOf("Home", "Delete")).toBeNull();
    expect(buttonOf("RDC", "Delete")).toBeTruthy();
  });

  it("opens the form with the root as parent from Add zone, and the row's zone from Add sub-zone", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Add zone" }));
    expect((screen.getByLabelText("Parent zone") as HTMLSelectElement).value).toBe("root");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(buttonOf("RDC", "Add sub-zone")!);
    expect((screen.getByLabelText("Parent zone") as HTMLSelectElement).value).toBe("rdc");
  });

  it("creates a zone with POST and reads the tree again", async () => {
    const api = setup({ "POST /api/v1/zones": { status: 201, body: zone("a", "Attic") } });
    fireEvent.click(screen.getByRole("button", { name: "Add zone" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Attic" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.key === "POST /api/v1/zones")!.body).toEqual({
      name: "Attic",
      parentId: "root",
    });
    expect(api.count("GET /api/v1/zones")).toBe(1);
  });

  it("edits a zone with PUT, the description included, and the root without parentId", async () => {
    const api = setup({
      "PUT /api/v1/zones/rdc": { body: {} },
      "PUT /api/v1/zones/root": { body: {} },
    });
    fireEvent.click(buttonOf("RDC", "Edit")!);
    expect(screen.getByRole("dialog", { name: "Edit zone" })).toBeTruthy();
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Ground" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.key === "PUT /api/v1/zones/rdc")!.body).toEqual({
      name: "Ground",
      parentId: "root",
      description: "",
    });
    fireEvent.click(buttonOf("Home", "Edit")!);
    expect(screen.queryByLabelText("Parent zone")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.key === "PUT /api/v1/zones/root")!.body).toEqual({
      name: "Home",
      description: "",
    });
  });

  it("deletes after the confirmation, and sends nothing when it is cancelled", async () => {
    const api = setup({ "DELETE /api/v1/zones/garage": { status: 204 } });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
    fireEvent.click(buttonOf("Garage", "Delete")!);
    expect(window.confirm).toHaveBeenCalledWith('Delete zone "Garage"? This cannot be undone.');
    expect(api.count("DELETE /api/v1/zones/garage")).toBe(0);
    fireEvent.click(buttonOf("Garage", "Delete")!);
    await waitFor(() => expect(api.count("DELETE /api/v1/zones/garage")).toBe(1));
  });

  it("shows the refusal under the row for 5 s", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const text = "Cannot delete zone with 1 child zone. Remove or move them first.";
    setup({ "DELETE /api/v1/zones/rdc": { status: 400, body: { error: text } } });
    await act(async () => {
      fireEvent.click(buttonOf("RDC", "Delete")!);
    });
    await advance(0);
    expect(screen.getByText(text)).toBeTruthy();
    await advance(4900);
    expect(screen.getByText(text)).toBeTruthy();
    await advance(200);
    expect(screen.queryByText(text)).toBeNull();
  });

  it("reads the tree again when the zone was already deleted", async () => {
    const api = setup({
      "DELETE /api/v1/zones/garage": { status: 404, body: { error: "Zone not found" } },
    });
    fireEvent.click(buttonOf("Garage", "Delete")!);
    expect(await screen.findByText("This item does not exist or was deleted.")).toBeTruthy();
    expect(api.count("GET /api/v1/zones")).toBe(1);
  });

  it("shows Sowel's error state when the tree cannot be read, and reads again on Retry", async () => {
    const api = mockApi({
      "GET /api/v1/zones": [{ status: 400, body: { error: "disk error" } }, { body: tree() }],
    });
    useZones.setState({ tree: [] });
    render(<ZonesPage />);
    await act(async () => {
      await useZones.getState().fetchZones();
    });
    expect(screen.getByText("Failed to load zones")).toBeTruthy();
    expect(screen.getByText("disk error")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Garage")).toBeTruthy();
    expect(api.count("GET /api/v1/zones")).toBe(2);
  });

  it("opens the room's page when a name is clicked", () => {
    setup();
    fireEvent.click(screen.getByText("Garage"));
    expect(window.location.hash).toBe("#/zone/garage");
  });
});
