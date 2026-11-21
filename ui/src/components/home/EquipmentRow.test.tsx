import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import {
  FakeWebSocket,
  homeRoutes,
  installFakeWebSocket,
  light,
  mockApi,
  resetStores,
  zone,
  advance,
} from "../../test-utils";
import { App } from "../../App";
import { useEquipments } from "../../store/useEquipments";
import { EquipmentRow } from "./EquipmentRow";

afterEach(() => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("light control (Sowel's LightControl, compact)", () => {
  beforeEach(() => {
    installFakeWebSocket();
    window.location.hash = "";
  });

  it("sends ON with the button disabled while it waits, and shows the new state only when the device confirms it", async () => {
    let answer: () => void = () => {};
    const pending = new Promise<void>((resolve) => (answer = resolve));
    const api = mockApi({
      ...homeRoutes({
        zones: [zone("root", "Home", [zone("garage", "Garage", [], "root")])],
        equipments: [light("l", "Garage light", "garage", false)],
      }),
      "POST /api/v1/equipments/l/orders/state": async () => {
        await pending;
        return { status: 204 };
      },
    });
    render(<App />);
    act(() => FakeWebSocket.last.open());
    const button = await screen.findByTitle("Turn on");
    fireEvent.click(button);
    await vi.waitFor(() => expect(api.count("POST /api/v1/equipments/l/orders/state")).toBe(1));
    expect(api.calls.find((c) => c.key.startsWith("POST"))!.body).toEqual({ value: "ON" });
    expect((screen.getByTitle("Turn on") as HTMLButtonElement).disabled).toBe(true);
    // The answer is in, the state is not: no optimistic value.
    await act(async () => answer());
    await vi.waitFor(() =>
      expect((screen.getByTitle("Turn on") as HTMLButtonElement).disabled).toBe(false),
    );
    expect(screen.queryByTitle("Turn off")).toBeNull();
    // The device's confirmation, through the socket, turns it on.
    act(() =>
      FakeWebSocket.last.receive([
        { type: "equipment.data.changed", equipmentId: "l", alias: "state", value: true },
      ]),
    );
    expect(await screen.findByTitle("Turn off")).toBeTruthy();
    expect(useEquipments.getState().equipments[0].dataBindings[0].value).toBe(true);
  });

  it("sends OFF for a light that is on", async () => {
    const execute = vi.fn(async () => {});
    render(<EquipmentRow equipment={light("l", "Light", "z", true)} onExecuteOrder={execute} />);
    fireEvent.click(screen.getByTitle("Turn off"));
    await vi.waitFor(() => expect(execute).toHaveBeenCalledWith("l", "state", "OFF"));
  });
});

describe("a refused order", () => {
  async function pressWith(error: Error) {
    vi.useFakeTimers();
    const execute = vi.fn(async () => {
      throw error;
    });
    render(<EquipmentRow equipment={light("l", "Light", "z", false)} onExecuteOrder={execute} />);
    fireEvent.click(screen.getByTitle("Turn on"));
    await advance(0);
  }

  it("shows the text of a 502 under the equipment, then removes it after 5 s", async () => {
    await pressWith(new ApiError(502, "No answer from the device"));
    expect(screen.getByRole("alert").textContent).toBe("No answer from the device");
    await advance(4999);
    expect(screen.getByRole("alert")).toBeTruthy();
    await advance(1);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows the text of a 503", async () => {
    await pressWith(new ApiError(503, "Busy"));
    expect(screen.getByRole("alert").textContent).toBe("Busy");
  });

  it("shows the translated text of a network error, not the browser's", async () => {
    await pressWith(new ApiError(0, "Failed to fetch"));
    expect(screen.getByRole("alert").textContent).toBe("Cannot reach Solite");
  });
});
