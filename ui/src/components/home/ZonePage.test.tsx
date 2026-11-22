import { cleanup, fireEvent, render, screen, within } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { aggregate, equipment, light, mockApi, resetStores, sensor, zone } from "../../test-utils";
import { useZones } from "../../store/useZones";
import { ZonePage } from "./ZonePage";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

const noOrder = vi.fn(async () => {});
const garage = zone("garage", "Garage", [], "root");

describe("ZonePage", () => {
  it("shows the zone name as h1, the pills of the aggregate and the lights line", () => {
    render(
      <ZonePage
        zone={garage}
        aggregate={aggregate({
          temperature: 12.6,
          humidity: 59,
          luminosity: 320,
          motion: true,
          motionSensors: 1,
          lightsTotal: 2,
          lightsOn: 1,
        })}
        equipments={[]}
        onExecuteOrder={noOrder}
      />,
    );
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Garage");
    // Sowel's units: a thin or no-break space before them, so the text is matched without it.
    const text = (document.body.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");
    expect(text).toContain("12.6°");
    expect(text).toContain("59%");
    expect(text).toContain("320 lx");
    expect(text).toContain("Motion");
    expect(screen.getByText("1/2")).toBeTruthy(); // the lights pill
    expect(screen.getByText("1 light on")).toBeTruthy(); // ZoneHeroLead
  });

  it("says all lights are off, and shows no pill that has no data", () => {
    render(
      <ZonePage
        zone={garage}
        aggregate={aggregate({ lightsTotal: 2, lightsOn: 0 })}
        equipments={[]}
        onExecuteOrder={noOrder}
      />,
    );
    expect(screen.getByText("all lights off")).toBeTruthy();
    expect(screen.queryByText("Motion")).toBeNull();
    expect(screen.queryByText("Calm")).toBeNull();
  });

  it("lists the equipments of the zone by family: lights, then sensors; a disabled one is dimmed without control", () => {
    render(
      <ZonePage
        zone={garage}
        aggregate={aggregate()}
        equipments={[
          sensor("s", "Garage climate", "garage"),
          light("l", "Garage light", "garage", true),
          light("d", "Old light", "garage", false, { enabled: false }),
        ]}
        onExecuteOrder={noOrder}
      />,
    );
    const titles = screen.getAllByText(/^(Lights|Sensors)$/).map((e) => e.textContent);
    expect(titles).toEqual(["Lights", "Sensors"]);
    const names = screen
      .getAllByText(/Garage light|Old light|Garage climate/)
      .map((e) => e.textContent);
    expect(names).toEqual(["Garage light", "Old light", "Garage climate"]);
    // The enabled light has its button; the disabled one is dimmed and has none.
    const old = screen.getByText("Old light").closest(".opacity-50") as HTMLElement;
    expect(old).toBeTruthy();
    expect(within(old).queryByRole("button")).toBeNull();
    const live = screen
      .getByText("Garage light")
      .closest("div[class*='slice-rows'] > div") as HTMLElement;
    expect(within(live).getByRole("button").getAttribute("title")).toBe("Turn off");
    // The sensor's values (Sowel's SensorValues).
    const climate = screen
      .getByText("Garage climate")
      .closest("div[class*='slice-rows'] > div") as HTMLElement;
    expect((climate.textContent ?? "").replace(/[\u202f\u00a0]/g, " ")).toContain("59%");
  });

  it("shows an offline equipment with Sowel's badge", () => {
    render(
      <ZonePage
        zone={garage}
        aggregate={aggregate()}
        equipments={[light("l", "Garage light", "garage", false, { status: "offline" })]}
        onExecuteOrder={noOrder}
      />,
    );
    expect(screen.getByRole("status").textContent).toBe("Disconnected");
  });

  it("gives a light_dimmable its name, icon and badge, and no control", () => {
    render(
      <ZonePage
        zone={garage}
        aggregate={aggregate()}
        equipments={[equipment("d", "Dimmer", "light_dimmable", "garage", { status: "degraded" })]}
        onExecuteOrder={noOrder}
      />,
    );
    const row = screen.getByText("Dimmer").closest("div[class*='slice-rows'] > div") as HTMLElement;
    expect(row.querySelector("svg")).toBeTruthy();
    expect(within(row).getByRole("status").textContent).toBe("Degraded");
    expect(within(row).queryByRole("button")).toBeNull();
  });

  it("only lists the equipments it is given, and says there is none", () => {
    render(
      <ZonePage zone={garage} aggregate={undefined} equipments={[]} onExecuteOrder={noOrder} />,
    );
    expect(screen.getByText("No equipment in this room")).toBeTruthy();
  });

  it("opens the equipment form with this room chosen from Add equipment", () => {
    mockApi({ "GET /api/v1/devices": { body: [] } });
    useZones.setState({ tree: [zone("root", "Home", [garage])] });
    render(
      <ZonePage zone={garage} aggregate={undefined} equipments={[]} onExecuteOrder={noOrder} />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Add equipment" }));
    expect(screen.getByRole("dialog", { name: "Create equipment" })).toBeTruthy();
    expect((screen.getByLabelText("Zone") as HTMLSelectElement).value).toBe("garage");
  });
});
