import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FakeWebSocket,
  homeRoutes,
  installFakeWebSocket,
  mockApi,
  resetStores,
  zone,
  advance,
} from "../../test-utils";
import { connect } from "../../store/useLive";
import { Banner } from "./Banner";

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  installFakeWebSocket();
  mockApi(homeRoutes({ zones: [zone("root", "Home")] }));
});

afterEach(() => {
  cleanup();
  resetStores();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const TEXT = "Connection lost, reconnecting…";

describe("Banner", () => {
  it("shows once the socket has been closed for 3 s, and hides when it opens", async () => {
    render(<Banner />);
    connect();
    act(() => FakeWebSocket.last.open());
    act(() => FakeWebSocket.last.drop());
    await advance(2999);
    expect(screen.queryByText(TEXT)).toBeNull();
    await advance(1);
    expect(screen.getByText(TEXT)).toBeTruthy();
    await advance(0);
    act(() => FakeWebSocket.last.open());
    expect(screen.queryByText(TEXT)).toBeNull();
  });

  it("shows when /ws is refused (a fifth tab, 503) and the page retries with the backoff", async () => {
    render(<Banner />);
    connect();
    act(() => FakeWebSocket.last.drop()); // the browser reports a refused upgrade as a close
    await advance(3000);
    expect(screen.getByText(TEXT)).toBeTruthy();
    expect(FakeWebSocket.instances.length).toBe(2); // the retry after 1 s
    act(() => FakeWebSocket.last.drop());
    await advance(1999);
    expect(FakeWebSocket.instances.length).toBe(2);
    await advance(1);
    expect(FakeWebSocket.instances.length).toBe(3); // after 2 s
    expect(screen.getByText(TEXT)).toBeTruthy();
  });

  it("shows nothing for a restart of the host that is over within 3 s", async () => {
    render(<Banner />);
    connect();
    act(() => FakeWebSocket.last.open());
    act(() => FakeWebSocket.last.drop());
    await advance(1000);
    act(() => FakeWebSocket.last.open());
    await advance(5000);
    expect(screen.queryByText(TEXT)).toBeNull();
  });
});
