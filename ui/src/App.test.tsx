import { render, screen } from "@testing-library/preact";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("shows the tool's name as the page title, beside the sidebar", () => {
    render(<App />);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("mini-sowel");
    expect(screen.getByRole("complementary")).toBeTruthy();
  });
});
