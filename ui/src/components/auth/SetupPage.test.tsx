import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { authTokens, mockApi, resetStores } from "../../test-utils";
import { useAuth } from "../../store/useAuth";
import { SetupPage } from "./SetupPage";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

const PASSWORD = "synthetic-pass-1";

function fill(password = PASSWORD, confirm = password) {
  const type = (label: string, value: string) =>
    fireEvent.input(screen.getByLabelText(label), { target: { value } });
  type("Username", "admin");
  type("Display name", "Admin");
  type("Password", password);
  type("Confirm password", confirm);
  fireEvent.click(screen.getByRole("button", { name: "Create account" }));
}

describe("SetupPage", () => {
  it("says the passwords do not match, without sending", async () => {
    const api = mockApi({});
    render(<SetupPage />);
    fill(PASSWORD, "another-one-12");
    expect(await screen.findByText("Passwords do not match")).toBeTruthy();
    expect(api.calls).toEqual([]);
  });

  it("refuses 7 characters and 129 characters, and counts Unicode code points", async () => {
    const api = mockApi({ "POST /api/v1/auth/setup": { status: 201, body: authTokens() } });
    render(<SetupPage />);
    fill("1234567");
    expect(await screen.findByText("Password must be at least 8 characters")).toBeTruthy();
    cleanup();
    render(<SetupPage />);
    fill("a".repeat(129));
    expect(await screen.findByText("Password must be at most 128 characters")).toBeTruthy();
    // 4 emoji are 8 UTF-16 units but 4 code points: too short.
    cleanup();
    render(<SetupPage />);
    fill("😀".repeat(4));
    expect(await screen.findByText("Password must be at least 8 characters")).toBeTruthy();
    expect(api.calls).toEqual([]);
    // 128 emoji are 256 UTF-16 units but 128 code points: accepted.
    cleanup();
    render(<SetupPage />);
    fill("😀".repeat(128));
    await waitFor(() => expect(api.count("POST /api/v1/auth/setup")).toBe(1));
  });

  it("shows the host's refusal under the form", async () => {
    mockApi({
      "POST /api/v1/auth/setup": {
        status: 403,
        body: { error: "Press the button on the box, then try again", confirmRequired: true },
      },
    });
    render(<SetupPage />);
    fill();
    expect(await screen.findByText("Press the button on the box, then try again")).toBeTruthy();
    expect(useAuth.getState().isAuthenticated).toBe(false);
  });

  it("on success keeps the tokens and opens the session", async () => {
    const api = mockApi({ "POST /api/v1/auth/setup": { status: 201, body: authTokens() } });
    render(<SetupPage />);
    fill();
    await waitFor(() => expect(useAuth.getState().isAuthenticated).toBe(true));
    expect(api.calls[0].body).toEqual({
      username: "admin",
      password: PASSWORD,
      displayName: "Admin",
      language: "en",
    });
    expect(useAuth.getState().setupRequired).toBe(false);
    expect(localStorage.getItem("sowel_access_token")).toBe("access-1");
    expect(localStorage.getItem("sowel_refresh_token")).toBe("refresh-1");
  });
});
