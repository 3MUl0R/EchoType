import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import App from "./App.svelte";

// Mock the Tauri core invoke function
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue("pong"),
}));

describe("App", () => {
  it("renders the app title", () => {
    render(App);
    expect(screen.getByText("EchoType")).toBeTruthy();
  });

  it("renders navigation items", () => {
    render(App);
    expect(screen.getByText("Dictation")).toBeTruthy();
    expect(screen.getByText("Settings")).toBeTruthy();
  });

  it("has accessible navigation", () => {
    render(App);
    const nav = screen.getByRole("navigation");
    expect(nav).toBeTruthy();
    expect(nav.getAttribute("aria-label")).toBe("Main navigation");
  });
});
