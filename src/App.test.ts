import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import App from "./App.svelte";

// Mock the Tauri core invoke function
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi
    .fn()
    .mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_dictation_state") return Promise.resolve("idle");
      if (cmd === "get_setting" && args?.key === "wizard_completed")
        return Promise.resolve("true");
      if (cmd === "check_permissions")
        return Promise.resolve({ accessibility: true, microphone: true });
      if (cmd === "list_available_models") return Promise.resolve([]);
      if (cmd === "check_for_update") return Promise.resolve(null);
      return Promise.resolve("pong");
    }),
}));

// Mock the Tauri event API
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("App", () => {
  it("renders the app title", async () => {
    render(App);
    expect(await screen.findByText("EchoType")).toBeTruthy();
  });

  it("renders navigation items", async () => {
    render(App);
    expect(await screen.findByText("Dictation")).toBeTruthy();
    expect(screen.getByText("Models")).toBeTruthy();
    expect(screen.getByText("History")).toBeTruthy();
    expect(screen.getByText("Settings")).toBeTruthy();
  });

  it("has accessible navigation", async () => {
    render(App);
    const nav = await screen.findByRole("navigation");
    expect(nav).toBeTruthy();
    expect(nav.getAttribute("aria-label")).toBe("Main navigation");
  });

  it("renders record button", async () => {
    render(App);
    expect(await screen.findByText("Click to Record")).toBeTruthy();
  });

  it("renders empty transcription state", async () => {
    render(App);
    expect(
      await screen.findByText("Record something to see it transcribed."),
    ).toBeTruthy();
  });

  it("has accessible transcription region", async () => {
    render(App);
    const section = await screen.findByRole("region", {
      name: "Transcription result",
    });
    expect(section).toBeTruthy();
    expect(section.getAttribute("aria-live")).toBe("polite");
  });

  it("does not show dictation status badge when idle", async () => {
    render(App);
    // Wait for the app to finish async rendering
    await screen.findByText("EchoType");
    expect(screen.queryByText("Recording")).toBeNull();
    expect(screen.queryByText("Transcribing")).toBeNull();
    expect(screen.queryByText("Inserting")).toBeNull();
  });
});
