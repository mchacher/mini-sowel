import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import { fixtures, mockApi } from "../../test-utils";
import { AddBindingModal } from "./AddBindingModal";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function setup(
  props: Partial<Parameters<typeof AddBindingModal>[0]> = {},
  devices = [fixtures.zbminir2(), fixtures.th01()],
) {
  const api = mockApi({
    "GET /api/v1/devices": { body: devices },
    ...Object.fromEntries(devices.map((d) => [`GET /api/v1/devices/${d.id}`, { body: d }])),
  });
  const onAdd = vi.fn(async () => {});
  const onClose = vi.fn();
  render(
    <AddBindingModal
      mode="data"
      existingAliases={[]}
      equipmentType="light_onoff"
      onAdd={onAdd}
      onClose={onClose}
      {...props}
    />,
  );
  return { api, onAdd, onClose };
}

const pickDevice = async (name: string) => {
  fireEvent.click(await screen.findByRole("button", { name: new RegExp(`^${name}`) }));
  await screen.findByRole("button", { name: "Back" });
};
const alias = () => screen.getByLabelText("Alias") as HTMLInputElement;
const add = () => screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;

describe("AddBindingModal (spec 030, R4)", () => {
  it("reads the devices, then the device, suggests the alias, and sends {id, alias}", async () => {
    const { api, onAdd, onClose } = setup();
    await pickDevice("ZBMINIR2");
    expect(api.count("GET /api/v1/devices/zb")).toBe(1);
    expect(add().disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    expect(alias().value).toBe("state");
    fireEvent.click(add());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onAdd).toHaveBeenCalledWith({ id: "zb-state", alias: "state" });
  });

  it("trims the alias, which stays editable", async () => {
    const { onAdd } = setup();
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    fireEvent.input(alias(), { target: { value: "  power  " } });
    fireEvent.click(add());
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith({ id: "zb-state", alias: "power" }));
  });

  it("lists the device's orders in order mode, and sends the order's id", async () => {
    const { onAdd } = setup({ mode: "order" });
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    fireEvent.click(add());
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith({ id: "zb-o-state", alias: "state" }));
  });

  it("refuses an alias already in use, sending nothing", async () => {
    const { onAdd, onClose } = setup({ existingAliases: ["state"] });
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    fireEvent.click(add());
    expect(await screen.findByText('Alias "state" already in use')).toBeTruthy();
    expect(onAdd).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("refuses an alias of 33 characters, sending nothing, and an empty one cannot be sent", async () => {
    const { onAdd } = setup();
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    fireEvent.input(alias(), { target: { value: "a".repeat(33) } });
    fireEvent.click(add());
    expect(await screen.findByText("At most 32 characters")).toBeTruthy();
    expect(onAdd).not.toHaveBeenCalled();
    fireEvent.input(alias(), { target: { value: "   " } });
    expect(add().disabled).toBe(true);
  });

  it("counts an alias of 32 emoji (64 UTF-16 units) as 32 characters", async () => {
    const { onAdd } = setup();
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    const emoji = "😀".repeat(32);
    fireEvent.input(alias(), { target: { value: emoji } });
    fireEvent.click(add());
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith({ id: "zb-state", alias: emoji }));
  });

  it("shows the host's refusal and stays open", async () => {
    const onAdd = vi.fn(async () => {
      throw new ApiError(400, "Too many data bindings (16)");
    });
    const { onClose } = setup({ onAdd });
    await pickDevice("ZBMINIR2");
    fireEvent.click(screen.getByRole("button", { name: /state/ }));
    fireEvent.click(add());
    expect(await screen.findByText("Too many data bindings (16)")).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
    expect(add().disabled).toBe(false);
  });

  it("says there is nothing to bind on a device with no order in order mode", async () => {
    setup({ mode: "order", equipmentType: "sensor" });
    await pickDevice("TH01");
    expect(screen.getByText("Nothing to bind on this device")).toBeTruthy();
  });

  it("says there is no device when there is none, and goes back to the devices", async () => {
    setup({}, []);
    expect(await screen.findByText("No device")).toBeTruthy();
  });

  it("shows the error, not No device, when the devices cannot be read", async () => {
    mockApi({ "GET /api/v1/devices": { status: 400, body: { error: "radio busy" } } });
    render(
      <AddBindingModal
        mode="data"
        existingAliases={[]}
        equipmentType="light_onoff"
        onAdd={async () => {}}
        onClose={() => {}}
      />,
    );
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByText(/No device/)).toBeNull();
  });

  it("goes back from step 2 to the devices", async () => {
    setup();
    await pickDevice("TH01");
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("button", { name: /^ZBMINIR2/ })).toBeTruthy();
  });

  it("closes on Escape and on a click outside", async () => {
    const { onClose } = setup();
    await screen.findByRole("button", { name: /^ZBMINIR2/ });
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
