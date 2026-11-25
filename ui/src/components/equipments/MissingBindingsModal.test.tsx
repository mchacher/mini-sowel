import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { dataBinding, fixtures, mockApi } from "../../test-utils";
import type { OrderBindingWithDetails } from "../../types";
import { MissingBindingsModal } from "./MissingBindingsModal";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const EQ = "/api/v1/equipments/e";

const order = (key: string): OrderBindingWithDetails => ({
  id: `ob-${key}`,
  equipmentId: "e",
  deviceOrderId: `bulb-o-${key}`,
  alias: key,
  deviceId: "bulb",
  deviceName: "Bulb",
  key,
  type: "number",
});

/** A dimmable bulb with its two orders bound and no data: `state` and `brightness` are missing. */
function setup(routes: Parameters<typeof mockApi>[0] = {}, bound = true) {
  const api = mockApi({
    "GET /api/v1/devices/bulb": { body: fixtures.bulb() },
    [`POST ${EQ}/data-bindings`]: { status: 201, body: {} },
    [`POST ${EQ}/order-bindings`]: { status: 201, body: {} },
    ...routes,
  });
  const onClose = vi.fn();
  const onAdded = vi.fn();
  const data = bound
    ? [
        dataBinding("state", "light_state", true, {
          deviceId: "bulb",
          deviceDataId: "bulb-state",
        }),
        dataBinding("brightness", "light_brightness", 1, {
          deviceId: "bulb",
          deviceDataId: "bulb-brightness",
        }),
      ]
    : [];
  render(
    <MissingBindingsModal
      equipmentId="e"
      equipmentType="light_dimmable"
      dataBindings={data}
      orderBindings={[order("state"), order("brightness")]}
      onClose={onClose}
      onAdded={onAdded}
    />,
  );
  return { api, onClose, onAdded };
}

const addButton = (count: number) =>
  screen.findByRole("button", { name: `Add ${count}` }) as Promise<HTMLButtonElement>;

describe("MissingBindingsModal (spec 030, R6)", () => {
  it("lists the missing ones all ticked, with alias, kind, device and key", async () => {
    setup({}, false);
    await addButton(2);
    expect((screen.getByLabelText("state") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("brightness") as HTMLInputElement).checked).toBe(true);
    expect(screen.getAllByText("(data)")).toHaveLength(2);
    expect(screen.getAllByText(/Bulb · /)).toHaveLength(2);
  });

  it("sends only the ticked ones and calls onAdded", async () => {
    const { api, onAdded } = setup({}, false);
    const button = await addButton(2);
    fireEvent.click(screen.getByLabelText("brightness"));
    expect(button.textContent).toBe("Add 1");
    fireEvent.click(button);
    await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
    expect(api.calls.filter((c) => c.key.startsWith("POST"))).toEqual([
      { key: `POST ${EQ}/data-bindings`, body: { deviceDataId: "bulb-state", alias: "state" } },
    ]);
  });

  it("disables Add with nothing ticked", async () => {
    setup({}, false);
    await addButton(2);
    fireEvent.click(screen.getByLabelText("state"));
    fireEvent.click(screen.getByLabelText("brightness"));
    expect(((await addButton(0)) as HTMLButtonElement).disabled).toBe(true);
  });

  it("says nothing is missing when everything is bound", async () => {
    setup();
    expect(await screen.findByText("Nothing missing")).toBeTruthy();
  });

  it("stops at the first refusal and shows its text, the dialog open", async () => {
    const { api, onAdded } = setup(
      {
        [`POST ${EQ}/data-bindings`]: { status: 400, body: { error: "Too many data bindings" } },
      },
      false,
    );
    fireEvent.click(await addButton(2));
    expect(await screen.findByText("Too many data bindings")).toBeTruthy();
    expect(api.calls.filter((c) => c.key.startsWith("POST"))).toHaveLength(1);
    expect(onAdded).not.toHaveBeenCalled();
  });

  it("sends only the items not yet written when Add is clicked again after a refusal", async () => {
    const { api, onAdded } = setup(
      {
        [`POST ${EQ}/data-bindings`]: [
          { status: 201, body: {} },
          { status: 400, body: { error: "Device item gone" } },
          { status: 201, body: {} },
        ],
      },
      false,
    );
    fireEvent.click(await addButton(2));
    expect(await screen.findByText("Device item gone")).toBeTruthy();
    // `state` was written and left the list: only `brightness` remains.
    expect(screen.queryByLabelText("state")).toBeNull();
    fireEvent.click(await addButton(1));
    await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
    expect(api.calls.filter((c) => c.key.startsWith("POST")).map((c) => c.body)).toEqual([
      { deviceDataId: "bulb-state", alias: "state" },
      { deviceDataId: "bulb-brightness", alias: "brightness" },
      { deviceDataId: "bulb-brightness", alias: "brightness" },
    ]);
  });

  it("closes on Escape", async () => {
    const { onClose } = setup({}, false);
    await addButton(2);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
