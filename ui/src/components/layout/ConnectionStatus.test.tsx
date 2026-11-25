// Ported from Sowel ui/src/components/layout/ConnectionStatus.test.tsx (Sowel commit 2d360726). Changed: the
// status is Solite's `useLive` (`open`, `connecting`, `closed`), not `useWebSocket`.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/preact";
import { setLanguage } from "../../i18n";
import { useLive } from "../../store/useLive";
import { ConnectionStatus } from "./ConnectionStatus";
import { DEGRADED_DELAY_MS } from "../../hooks/useTopBarState";

// Spec 196, FR-6 — the link indicator shows only when the link is degraded:
// no « Connecté » pill and no permanent ping while all is well, and no flash
// during the routine disconnected → connecting → connected cycle.

describe("ConnectionStatus", () => {
  beforeEach(() => {
    setLanguage("fr");
    vi.useFakeTimers();
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    setLanguage("en");
    useLive.setState({ status: "connecting" });
  });

  it("renders nothing while connected", () => {
    useLive.setState({ status: "open" });
    const { container } = render(<ConnectionStatus />);
    act(() => {
      vi.advanceTimersByTime(DEGRADED_DELAY_MS * 2);
    });
    expect(container.firstChild).toBeNull();
  });

  it("shows « Connexion… » only once the reconnection has lasted 3 s", () => {
    useLive.setState({ status: "connecting" });
    render(<ConnectionStatus />);
    act(() => {
      vi.advanceTimersByTime(DEGRADED_DELAY_MS - 1);
    });
    expect(screen.queryByRole("status")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    const indicator = screen.getByRole("status", { name: "Connexion…" });
    expect(indicator.innerHTML).not.toMatch(/animate-/);
  });

  it("names a lost link, and hides again once it is back", () => {
    useLive.setState({ status: "closed" });
    render(<ConnectionStatus />);
    act(() => {
      vi.advanceTimersByTime(DEGRADED_DELAY_MS);
    });
    expect(screen.getByRole("status", { name: "Déconnecté" })).toBeTruthy();
    act(() => useLive.setState({ status: "open" }));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("does not show a quick reconnection at all", () => {
    useLive.setState({ status: "closed" });
    render(<ConnectionStatus />);
    act(() => {
      vi.advanceTimersByTime(500);
    });
    act(() => useLive.setState({ status: "connecting" }));
    act(() => {
      vi.advanceTimersByTime(500);
    });
    act(() => useLive.setState({ status: "open" }));
    act(() => {
      vi.advanceTimersByTime(DEGRADED_DELAY_MS * 2);
    });
    expect(screen.queryByRole("status")).toBeNull();
  });
});
