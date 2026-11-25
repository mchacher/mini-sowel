import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useEquipments } from "../../store/useEquipments";
import { light, mockApi, resetStores } from "../../test-utils";
import { EnabledRow } from "./EnabledRow";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

/** The row as the page gives it: `enabled` follows the store. */
function Harness() {
  const equipment = useEquipments((s) => s.equipments[0]);
  return <EnabledRow equipmentId="l" enabled={equipment.enabled} />;
}

function setup(routes: Parameters<typeof mockApi>[0]) {
  const api = mockApi({ "GET /api/v1/equipments": { body: [] }, ...routes });
  useEquipments.setState({ equipments: [light("l", "Lamp", "garage", false)] });
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: /Activation/ }));
  return api;
}

const toggle = () => screen.getByRole("switch") as HTMLButtonElement;

describe("EnabledRow (decision D50)", () => {
  it("sends enabled false when switched off, and says Disabled once the host answered", async () => {
    let answer: () => void = () => {};
    const pending = new Promise<void>((resolve) => (answer = resolve));
    const api = setup({
      "PUT /api/v1/equipments/l": async () => {
        await pending;
        useEquipments.setState({
          equipments: [light("l", "Lamp", "garage", false, { enabled: false })],
        });
        return { body: {} };
      },
    });
    expect(screen.getByText(/· Enabled/)).toBeTruthy();
    expect(toggle().getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle());
    // The switch waits for the answer: still on, and not clickable meanwhile.
    expect(toggle().getAttribute("aria-checked")).toBe("true");
    expect(toggle().disabled).toBe(true);
    answer();
    await waitFor(() => expect(screen.getByText(/· Disabled/)).toBeTruthy());
    expect(toggle().getAttribute("aria-checked")).toBe("false");
    expect(api.calls.find((c) => c.key === "PUT /api/v1/equipments/l")!.body).toEqual({
      enabled: false,
    });
  });

  it("shows the host's text on a refusal and the switch goes back to on", async () => {
    setup({ "PUT /api/v1/equipments/l": { status: 400, body: { error: "Cannot disable it" } } });
    fireEvent.click(toggle());
    expect(await screen.findByText("Cannot disable it")).toBeTruthy();
    expect(toggle().getAttribute("aria-checked")).toBe("true");
    expect(toggle().disabled).toBe(false);
  });
});
