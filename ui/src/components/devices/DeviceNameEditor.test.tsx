// Ported from Sowel ui/src/components/devices/DeviceNameEditor.test.tsx (Sowel commit 5da84dc1).
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import i18n, { setLanguage } from "../../i18n";
import { ApiError } from "../../api/client";
import { DeviceNameEditor } from "./DeviceNameEditor";

async function renameTo(onSave: (name: string) => Promise<void>, title: string) {
  render(<DeviceNameEditor name="Old" onSave={onSave} />);
  fireEvent.click(screen.getByTitle(title));
  const input = screen.getByRole("textbox");
  fireEvent.input(input, { target: { value: "New" } });
  fireEvent.keyDown(input, { key: "Enter" });
}

describe("DeviceNameEditor failure (#1124)", () => {
  afterEach(() => {
    cleanup();
    setLanguage("en");
  });

  it("calls a 429 a busy server, in French", async () => {
    setLanguage("fr");
    await renameTo(
      vi.fn().mockRejectedValue(new ApiError(429, "Too Many Requests")),
      "Modifier le nom",
    );
    expect(await screen.findByText(i18n.t("errors.rateLimited.message"))).toBeTruthy();
  });

  it("keeps the server's message for a refused name", async () => {
    await renameTo(vi.fn().mockRejectedValue(new ApiError(400, "Name is required")), "Edit name");
    expect(await screen.findByText("Name is required")).toBeTruthy();
  });

  it("falls back to the translated save failure", async () => {
    await renameTo(vi.fn().mockRejectedValue(new TypeError("boom")), "Edit name");
    expect(await screen.findByText("Failed to save")).toBeTruthy();
  });

  it("saves on Enter and cancels on Escape", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<DeviceNameEditor name="Old" onSave={onSave} />);
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("textbox")).toBeNull();
    fireEvent.click(screen.getByTitle("Edit name"));
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "New" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("New"));
  });
});
