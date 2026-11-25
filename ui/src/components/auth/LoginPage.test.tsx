import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { authTokens, mockApi, resetStores } from "../../test-utils";
import { useAuth } from "../../store/useAuth";
import { LoginPage } from "./LoginPage";

afterEach(() => {
  cleanup();
  resetStores();
  vi.unstubAllGlobals();
});

function submit() {
  fireEvent.input(screen.getByLabelText("Username"), { target: { value: "admin" } });
  fireEvent.input(screen.getByLabelText("Password"), { target: { value: "synthetic-pass-1" } });
  fireEvent.click(screen.getByRole("button", { name: "Log in" }));
}

describe("LoginPage", () => {
  it("shows Invalid credentials on a wrong password", async () => {
    mockApi({ "POST /api/v1/auth/login": { status: 401, body: { error: "Invalid credentials" } } });
    render(<LoginPage />);
    submit();
    expect(await screen.findByText("Invalid credentials")).toBeTruthy();
    expect(useAuth.getState().isAuthenticated).toBe(false);
  });

  it("on success keeps the tokens and opens the session", async () => {
    const api = mockApi({ "POST /api/v1/auth/login": { body: authTokens() } });
    render(<LoginPage />);
    submit();
    await waitFor(() => expect(useAuth.getState().isAuthenticated).toBe(true));
    expect(api.calls[0].body).toEqual({ username: "admin", password: "synthetic-pass-1" });
    expect(localStorage.getItem("sowel_access_token")).toBe("access-1");
  });
});
