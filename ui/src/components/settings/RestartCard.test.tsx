import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi, resetStores, type Route } from "../../test-utils";
import { RestartCard } from "./RestartCard";

const RESTART = "POST /api/v1/system/restart";
const VERSION = "GET /api/v1/system/version";
const INFO = { current: "0.1.0", latest: null, updateAvailable: false, releaseUrl: null };

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  resetStores();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const tick = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });

/** What the version route answers while the box restarts: still up, away, back (or 401). */
function box(routes: Record<string, Route | Route[]> = {}) {
  const state = { mode: "up" as "up" | "down" | "login" };
  const api = mockApi({
    [RESTART]: { status: 200, body: { success: true, message: "Restarting" } },
    [VERSION]: async () => {
      if (state.mode === "down") throw new TypeError("network down");
      if (state.mode === "login")
        return { status: 401, body: { error: "Authentication required" } };
      return { body: INFO };
    },
    ...routes,
  });
  return { state, api };
}

const restartButton = () => screen.getByRole("button", { name: "Restart" }) as HTMLButtonElement;
const confirm = () =>
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Restart" }));

describe("RestartCard", () => {
  it("Cancel sends nothing", () => {
    const { api } = box();
    const reload = vi.fn();
    render(<RestartCard reload={reload} />);
    fireEvent.click(restartButton());
    expect(screen.getByText("Restart the box? It is back in about a minute.")).toBeTruthy();
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.calls).toEqual([]);
    expect(reload).not.toHaveBeenCalled();
  });

  it("confirms, waits while the box is away, and reloads once it answers", async () => {
    const { state, api } = box();
    const reload = vi.fn();
    render(<RestartCard reload={reload} />);
    vi.useFakeTimers();
    fireEvent.click(restartButton());
    confirm();
    await tick(0);
    expect(api.count(RESTART)).toBe(1);
    expect(screen.getByText("The box is restarting… the page reloads by itself.")).toBeTruthy();
    expect(restartButton().disabled).toBe(true);
    // Still up while it stops its engine: not a reason to reload.
    await tick(2000);
    expect(reload).not.toHaveBeenCalled();
    state.mode = "down";
    await tick(4000);
    expect(reload).not.toHaveBeenCalled();
    state.mode = "up";
    await tick(2000);
    expect(reload).toHaveBeenCalledTimes(1);
    const polls = api.count(VERSION);
    await tick(10_000);
    expect(api.count(VERSION)).toBe(polls); // the polling stopped
  });

  it("counts a 401 as an answer: the session ended with the restart", async () => {
    const { state } = box();
    const reload = vi.fn();
    render(<RestartCard reload={reload} />);
    vi.useFakeTimers();
    fireEvent.click(restartButton());
    confirm();
    await tick(0);
    state.mode = "down";
    await tick(4000);
    state.mode = "login";
    await tick(2000);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("says so after 90 seconds without an answer, the button back, the polling stopped", async () => {
    const { state, api } = box();
    const reload = vi.fn();
    render(<RestartCard reload={reload} />);
    vi.useFakeTimers();
    fireEvent.click(restartButton());
    confirm();
    await tick(0);
    state.mode = "down";
    await tick(80_000);
    expect(screen.queryByText(/has not answered/)).toBeNull();
    await tick(12_000);
    expect(
      screen.getByText("The box has not answered for 90 seconds: reload the page."),
    ).toBeTruthy();
    expect(restartButton().disabled).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    const polls = api.count(VERSION);
    await tick(30_000);
    expect(api.count(VERSION)).toBe(polls);
  });

  it("shows the text of a 409 and enables the button again, no polling", async () => {
    const { api } = box({
      [RESTART]: {
        status: 409,
        body: { error: "The update is being checked, restart in a minute" },
      },
    });
    render(<RestartCard reload={vi.fn()} />);
    fireEvent.click(restartButton());
    confirm();
    expect(
      await screen.findByText("The update is being checked, restart in a minute"),
    ).toBeTruthy();
    expect(restartButton().disabled).toBe(false);
    expect(api.count(VERSION)).toBe(0);
  });

  it("shows the host's 501 text", async () => {
    box({ [RESTART]: { status: 501, body: { error: "The host does not restart itself" } } });
    render(<RestartCard reload={vi.fn()} />);
    fireEvent.click(restartButton());
    confirm();
    expect(await screen.findByText("The host does not restart itself")).toBeTruthy();
    expect(restartButton().disabled).toBe(false);
  });

  it("speaks French in a French browser", async () => {
    vi.spyOn(navigator, "language", "get").mockReturnValue("fr-FR");
    const { setLanguage, detectLanguage } = await import("../../i18n");
    setLanguage(detectLanguage());
    try {
      box();
      render(<RestartCard reload={vi.fn()} />);
      expect(screen.getByRole("heading", { name: "Redémarrer" })).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Redémarrer" }));
      expect(
        screen.getByText("Redémarrer le boîtier ? Il revient dans environ une minute."),
      ).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Annuler" }));
      expect(screen.queryByRole("dialog")).toBeNull();
    } finally {
      setLanguage("en");
    }
  });
});
