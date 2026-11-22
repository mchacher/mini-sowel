import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/client";
import { flattenZonesWithPath } from "../../lib/zone-path";
import { zone } from "../../test-utils";
import { ZoneForm, flattenZoneTree } from "./ZoneForm";

afterEach(cleanup);

// Home > RDC > Garage, Home > House
const tree = [
  zone("root", "Home", [
    zone("rdc", "RDC", [zone("garage", "Garage", [], "rdc")], "root"),
    zone("house", "House", [], "root"),
  ]),
];
const options = flattenZonesWithPath(tree);

function setup(extra: Partial<Parameters<typeof ZoneForm>[0]> = {}) {
  const onSubmit = vi.fn(async () => {});
  const onClose = vi.fn();
  render(
    <ZoneForm
      title="Create zone"
      parentZones={options}
      onSubmit={onSubmit}
      onClose={onClose}
      {...extra}
    />,
  );
  return { onSubmit, onClose };
}

const nameInput = () => screen.getByLabelText("Name") as HTMLInputElement;
const type = (el: HTMLElement, value: string) => fireEvent.input(el, { target: { value } });
const submit = () => screen.getByRole("button", { name: "Create" });

describe("ZoneForm", () => {
  it("keeps the button disabled while the name is blank, and stops the input at 64 and 500", () => {
    setup();
    expect((submit() as HTMLButtonElement).disabled).toBe(true);
    type(nameInput(), "   ");
    expect((submit() as HTMLButtonElement).disabled).toBe(true);
    expect(nameInput().maxLength).toBe(64);
    type(nameInput(), "n".repeat(64));
    expect(nameInput().value).toHaveLength(64);
    expect((submit() as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByLabelText(/Description/) as HTMLInputElement).maxLength).toBe(500);
  });

  it("creates under the root by default, with no None option", async () => {
    const { onSubmit, onClose } = setup();
    const select = screen.getByLabelText("Parent zone") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toEqual([
      "root",
      "rdc",
      "garage",
      "house",
    ]);
    expect(screen.queryByText(/None/)).toBeNull();
    type(nameInput(), " Attic ");
    fireEvent.click(submit());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith({ name: "Attic", parentId: "root", description: "" });
  });

  it("takes the parent it is given", async () => {
    const { onSubmit } = setup({ defaultParentId: "rdc" });
    type(nameInput(), "Shelf");
    fireEvent.click(submit());
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith({ name: "Shelf", parentId: "rdc", description: "" });
  });

  it("leaves the zone and its descendants out of its parent select when editing", () => {
    setup({
      initial: { name: "RDC" },
      parentZones: flattenZoneTree(tree, "rdc"),
      defaultParentId: "root",
    });
    const select = screen.getByLabelText("Parent zone") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toEqual(["root", "house"]);
    expect(select.value).toBe("root");
  });

  it("shows no parent select for the root and sends no parentId", async () => {
    const { onSubmit } = setup({ initial: { name: "Home" }, isRoot: true });
    expect(screen.queryByLabelText("Parent zone")).toBeNull();
    type(nameInput(), "My home");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith({ name: "My home", description: "" });
  });

  it.each(["Invalid description", "A zone cannot be its own parent", "Too many zones (32)"])(
    "stays open and shows the API's text: %s",
    async (text) => {
      const { onClose } = setup({
        onSubmit: async () => {
          throw new ApiError(400, text);
        },
      });
      type(nameInput(), "Attic");
      fireEvent.click(submit());
      expect(await screen.findByText(text)).toBeTruthy();
      expect(onClose).not.toHaveBeenCalled();
    },
  );

  it.each(["Parent zone not found: abc", "Zone not found"])(
    "shows the server's text for a 404: %s",
    async (text) => {
      setup({
        onSubmit: async () => {
          throw new ApiError(404, text);
        },
      });
      type(nameInput(), "Attic");
      fireEvent.click(submit());
      expect(await screen.findByText(text)).toBeTruthy();
    },
  );

  it("keeps the generic text for a 404 with no server message", async () => {
    setup({
      onSubmit: async () => {
        throw new ApiError(404, "HTTP 404", { fromServer: false });
      },
    });
    type(nameInput(), "Attic");
    fireEvent.click(submit());
    expect(await screen.findByText("This item does not exist or was deleted.")).toBeTruthy();
  });

  it("closes on Escape, on the cross and on a click outside, sending nothing", () => {
    const { onSubmit, onClose } = setup();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(2);
    fireEvent.click(screen.getByRole("dialog").parentElement!);
    expect(onClose).toHaveBeenCalledTimes(3);
    fireEvent.click(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalledTimes(3);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
