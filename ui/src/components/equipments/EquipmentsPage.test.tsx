import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  advance,
  equipment,
  light,
  mockApi,
  resetStores,
  sensor,
  zone,
  type Route,
} from "../../test-utils";
import { useEquipments } from "../../store/useEquipments";
import { useZones } from "../../store/useZones";
import type { EquipmentWithDetails } from "../../types";
import { EquipmentsPage } from "./EquipmentsPage";

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

function setup(equipments: EquipmentWithDetails[], routes: Record<string, Route | Route[]> = {}) {
  const api = mockApi({
    "GET /api/v1/equipments": { body: equipments },
    "GET /api/v1/devices": { body: [] },
    ...routes,
  });
  useZones.setState({ tree: tree() });
  useEquipments.setState({ equipments });
  render(<EquipmentsPage />);
  return api;
}

const rowOf = (name: string) => screen.getByText(name).closest("[data-testid]") as HTMLElement;
const garageLight = () => light("l", "Lumière garage", "garage", false);

describe("EquipmentsPage", () => {
  it("groups the equipments by room with the path, each row with type, status and devices", () => {
    setup([
      garageLight(),
      sensor("s", "Home climate", "root", { status: "offline" }),
      equipment("e", "Spare", "switch", "garage"),
    ]);
    expect(screen.getByRole("heading", { level: 1, name: "Equipments" })).toBeTruthy();
    expect(screen.getByText("3 equipments")).toBeTruthy();
    const headings = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(headings).toEqual(["Home", "RDC › Garage"]);
    const row = rowOf("Lumière garage");
    expect(within(row).getByText("Light (On/Off)")).toBeTruthy();
    expect(within(row).getByText("1 device")).toBeTruthy();
    expect(within(rowOf("Home climate")).getByRole("status")).toBeTruthy();
    expect(within(rowOf("Spare")).getByText("0 devices")).toBeTruthy();
  });

  it("opens the room's page when the name is clicked", () => {
    setup([garageLight()]);
    fireEvent.click(screen.getByText("Lumière garage"));
    expect(window.location.hash).toBe("#/zone/garage");
  });

  it("dims a disabled equipment", () => {
    setup([light("d", "Old light", "garage", false, { enabled: false }), garageLight()]);
    expect(rowOf("Old light").className).toContain("opacity-50");
    expect(rowOf("Lumière garage").className).not.toContain("opacity-50");
  });

  it("shows Sowel's empty state with its create button", () => {
    setup([]);
    expect(screen.getByText("No equipments yet")).toBeTruthy();
    expect(screen.getByText("Create your first equipment and bind it to a device.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Create equipment" }));
    expect(screen.getByRole("dialog", { name: "Create equipment" })).toBeTruthy();
  });

  it("opens the create form from Add equipment", () => {
    setup([garageLight()]);
    fireEvent.click(screen.getByRole("button", { name: "Add equipment" }));
    expect(screen.getByRole("dialog", { name: "Create equipment" })).toBeTruthy();
  });

  it("edits the name and the room with a PUT", async () => {
    const api = setup([garageLight()], { "PUT /api/v1/equipments/l": { body: {} } });
    fireEvent.click(within(rowOf("Lumière garage")).getByRole("button", { name: "Edit" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Plafonnier" } });
    fireEvent.change(screen.getByLabelText("Zone"), { target: { value: "root" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.calls.find((c) => c.key === "PUT /api/v1/equipments/l")!.body).toEqual({
      name: "Plafonnier",
      zoneId: "root",
    });
  });

  it("deletes after the confirmation, and sends nothing when it is cancelled", async () => {
    const api = setup([garageLight()], { "DELETE /api/v1/equipments/l": { status: 204 } });
    (window.confirm as ReturnType<typeof vi.fn>).mockReturnValueOnce(false);
    const button = () => within(rowOf("Lumière garage")).getByRole("button", { name: "Delete" });
    fireEvent.click(button());
    expect(window.confirm).toHaveBeenCalledWith(
      'Delete equipment "Lumière garage"? This cannot be undone.',
    );
    expect(api.count("DELETE /api/v1/equipments/l")).toBe(0);
    fireEvent.click(button());
    await waitFor(() => expect(api.count("DELETE /api/v1/equipments/l")).toBe(1));
    await waitFor(() => expect(api.count("GET /api/v1/equipments")).toBe(1));
  });

  it("shows a refusal for 5 s", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    setup([garageLight()], {
      "DELETE /api/v1/equipments/l": { status: 400, body: { error: "Cannot delete it" } },
    });
    await act(async () => {
      fireEvent.click(within(rowOf("Lumière garage")).getByRole("button", { name: "Delete" }));
    });
    await advance(0);
    expect(screen.getByText("Cannot delete it")).toBeTruthy();
    await advance(5100);
    expect(screen.queryByText("Cannot delete it")).toBeNull();
  });

  it("reads the list again when an edited equipment was already deleted", async () => {
    const api = setup([garageLight()], {
      "PUT /api/v1/equipments/l": { status: 404, body: { error: "Equipment not found" } },
    });
    fireEvent.click(within(rowOf("Lumière garage")).getByRole("button", { name: "Edit" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Plafonnier" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Equipment not found")).toBeTruthy();
    expect(api.count("GET /api/v1/equipments")).toBe(1);
  });

  it("reads the list again when the equipment was already deleted", async () => {
    const api = setup([garageLight()], {
      "DELETE /api/v1/equipments/l": { status: 404, body: { error: "Equipment not found" } },
    });
    fireEvent.click(within(rowOf("Lumière garage")).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("This item does not exist or was deleted.")).toBeTruthy();
    expect(api.count("GET /api/v1/equipments")).toBe(1);
  });
});
