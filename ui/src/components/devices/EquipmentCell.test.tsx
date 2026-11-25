import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { EquipmentCell } from "./EquipmentCell";

afterEach(() => {
  cleanup();
  window.location.hash = "";
});

describe("EquipmentCell (spec 030, R2)", () => {
  it("links the equipment's name to its page, the room to its own", () => {
    render(
      <EquipmentCell
        equipments={[{ id: "eq 1", name: "Lumière", zoneId: "garage", zoneLabel: "Garage" }]}
      />,
    );
    const name = screen.getByRole("link", { name: "Lumière" });
    expect(name.getAttribute("href")).toBe("#/equipments/eq%201");
    expect(screen.getByRole("link", { name: "Garage" }).getAttribute("href")).toBe("#/zone/garage");
    fireEvent.click(name);
    expect(window.location.hash).toBe("#/equipments/eq%201");
  });

  it("shows a dash when no equipment uses the device", () => {
    render(<EquipmentCell equipments={[]} />);
    expect(screen.getByText("—")).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
  });
});
