import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { advance, mockApi, resetStores } from "../test-utils";
import { usePairing } from "./usePairing";

const ROUTE = "/api/v1/integrations/solite/permit-join";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  resetStores();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("pairing store", () => {
  it("start opens the window from the answer and counts down each second", async () => {
    const api = mockApi({
      [`POST ${ROUTE}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${ROUTE}`]: { body: { permitJoin: true, remaining: 178 } },
    });
    await usePairing.getState().start();
    expect(api.calls[0]).toEqual({ key: `POST ${ROUTE}`, body: { time: 180 } });
    expect(usePairing.getState()).toMatchObject({ open: true, remaining: 180 });
    await advance(500);
    expect(usePairing.getState().remaining).toBe(180 - 0);
    await advance(500); // 1 s: the state is read again, and the countdown follows the host
    expect(api.count(`GET ${ROUTE}`)).toBe(1);
    expect(usePairing.getState().remaining).toBe(178);
    await advance(3000);
    expect(usePairing.getState().remaining).toBe(175);
  });

  it("reads the state once at 0, and closes", async () => {
    const api = mockApi({
      [`POST ${ROUTE}`]: { status: 202, body: { permitJoin: true, remaining: 3 } },
      [`GET ${ROUTE}`]: [
        { body: { permitJoin: true, remaining: 3 } },
        { body: { permitJoin: false, remaining: 0 } },
      ],
    });
    await usePairing.getState().start();
    await advance(1000); // the check one second after the POST: counts again from 3
    expect(api.count(`GET ${ROUTE}`)).toBe(1);
    await advance(3000);
    expect(api.count(`GET ${ROUTE}`)).toBe(2);
    expect(usePairing.getState()).toMatchObject({ open: false, remaining: 0 });
  });

  it("shows the radio's refusal read one second after start", async () => {
    mockApi({
      [`POST ${ROUTE}`]: { status: 202, body: { permitJoin: true, remaining: 180 } },
      [`GET ${ROUTE}`]: {
        body: { permitJoin: false, remaining: 0, error: "the radio refused permitJoining" },
      },
    });
    await usePairing.getState().start();
    await advance(1000);
    expect(usePairing.getState()).toMatchObject({
      open: false,
      error: "the radio refused permitJoining",
    });
  });

  it("stop posts time 0 and closes", async () => {
    const api = mockApi({
      [`POST ${ROUTE}`]: [
        { status: 202, body: { permitJoin: true, remaining: 180 } },
        { status: 202, body: { permitJoin: false, remaining: 0 } },
      ],
      [`GET ${ROUTE}`]: { body: { permitJoin: true, remaining: 179 } },
    });
    await usePairing.getState().start();
    await usePairing.getState().stop();
    expect(api.calls.filter((c) => c.key === `POST ${ROUTE}`)[1].body).toEqual({ time: 0 });
    expect(usePairing.getState()).toMatchObject({ open: false, remaining: 0 });
  });
});
