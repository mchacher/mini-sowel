import { afterEach, describe, expect, it, vi } from "vitest";
import { mockApi } from "../test-utils";
import { getPermitJoin, setPermitJoin } from "./pairing";

afterEach(() => vi.unstubAllGlobals());

describe("pairing api", () => {
  it("posts the time to the permit-join route and parses the answer", async () => {
    const api = mockApi({
      "POST /api/v1/integrations/solite/permit-join": {
        status: 202,
        body: { permitJoin: true, remaining: 180 },
      },
    });
    expect(await setPermitJoin(180)).toEqual({ permitJoin: true, remaining: 180 });
    expect(api.calls).toEqual([
      { key: "POST /api/v1/integrations/solite/permit-join", body: { time: 180 } },
    ]);
  });

  it("reads the state, with the radio's refusal", async () => {
    mockApi({
      "GET /api/v1/integrations/solite/permit-join": {
        body: { permitJoin: false, remaining: 0, error: "the radio refused" },
      },
    });
    expect(await getPermitJoin()).toEqual({
      permitJoin: false,
      remaining: 0,
      error: "the radio refused",
    });
  });
});
