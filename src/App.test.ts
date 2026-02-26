import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import App from "./App.svelte";

// Mock the Tauri core invoke function
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_dictation_state") return Promise.resolve("idle");
    return Promise.resolve("pong");
  }),
}));

// Mock the Tauri event API
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("App", () => {
  it("renders the app title", () => {
    render(App);
    expect(screen.getByText("EchoType")).toBeTruthy();
  });

  it("renders navigation items", () => {
    render(App);
    expect(screen.getByText("Dictation")).toBeTruthy();
    expect(screen.getByText("Models")).toBeTruthy();
  });

  it("has accessible navigation", () => {
    render(App);
    const nav = screen.getByRole("navigation");
    expect(nav).toBeTruthy();
    expect(nav.getAttribute("aria-label")).toBe("Main navigation");
  });

  it("renders record button", () => {
    render(App);
    expect(screen.getByText("Click to Record")).toBeTruthy();
  });

  it("renders empty transcription state", () => {
    render(App);
    expect(
      screen.getByText("Record something to see it transcribed."),
    ).toBeTruthy();
  });

  it("has accessible transcription region", () => {
    render(App);
    const section = screen.getByRole("region", {
      name: "Transcription result",
    });
    expect(section).toBeTruthy();
    expect(section.getAttribute("aria-live")).toBe("polite");
  });

  it("does not show dictation status badge when idle", () => {
    render(App);
    expect(screen.queryByText("Recording")).toBeNull();
    expect(screen.queryByText("Transcribing")).toBeNull();
    expect(screen.queryByText("Inserting")).toBeNull();
  });
});
