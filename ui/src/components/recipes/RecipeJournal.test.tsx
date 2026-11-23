import { cleanup, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi } from "../../test-utils";
import type { RecipeLogEntry } from "../../types";
import { RecipeJournal } from "./RecipeJournal";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const LOG = "GET /api/v1/recipe-instances/i1/log?limit=50";
const entry = (id: number, level: RecipeLogEntry["level"], message: string): RecipeLogEntry => ({
  id,
  instanceId: "i1",
  timestamp: "2026-11-23T09:41:00",
  message,
  level,
});

describe("RecipeJournal", () => {
  it("lists the lines in the host's order, errors and warnings in their colours", async () => {
    mockApi({
      [LOG]: {
        body: [
          entry(3, "error", "Order to Garage light failed"),
          entry(2, "warn", "Equipment removed from settings: Old"),
          entry(1, "info", "Instance created"),
        ],
      },
    });
    render(<RecipeJournal instanceId="i1" />);
    await waitFor(() => expect(screen.getByText("Instance created")).toBeTruthy());
    const items = screen.getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      "09:41Order to Garage light failed",
      "09:41Equipment removed from settings: Old",
      "09:41Instance created",
    ]);
    expect(items[0].className).toContain("text-error");
    expect(items[1].className).toContain("text-warning-ink");
    expect(items[2].className).not.toContain("text-error");
  });

  it("says No entries when the log is empty", async () => {
    mockApi({ [LOG]: { body: [] } });
    render(<RecipeJournal instanceId="i1" />);
    await waitFor(() => expect(screen.getByText("No entries")).toBeTruthy());
  });

  it("says the journal could not be read when the first read fails", async () => {
    mockApi({ [LOG]: { status: 500, body: { error: "boom" } } });
    render(<RecipeJournal instanceId="i1" />);
    await waitFor(() => expect(screen.getByText("The journal could not be read")).toBeTruthy());
  });

  it("reads again every 5 s, and a failed refresh keeps the lines shown", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const api = mockApi({
      [LOG]: [{ body: [entry(1, "info", "Instance created")] }, { status: 500, body: {} }],
    });
    render(<RecipeJournal instanceId="i1" />);
    await waitFor(() => expect(screen.getByText("Instance created")).toBeTruthy());
    expect(api.count(LOG)).toBe(1);
    await vi.advanceTimersByTimeAsync(5000);
    await waitFor(() => expect(api.count(LOG)).toBe(2));
    expect(screen.getByText("Instance created")).toBeTruthy();
    expect(screen.queryByText("The journal could not be read")).toBeNull();
  });
});
