import { invoke } from "@tauri-apps/api/core";

export type Theme = "dark" | "light" | "high-contrast" | "auto";

const THEME_CLASSES = ["dark", "light", "high-contrast"] as const;

/** Resolve "auto" to a concrete theme using OS media queries. */
function resolveAuto(): "dark" | "light" | "high-contrast" {
  if (
    globalThis.matchMedia &&
    globalThis.matchMedia("(prefers-contrast: more)").matches
  ) {
    return "high-contrast";
  }
  if (
    globalThis.matchMedia &&
    globalThis.matchMedia("(prefers-color-scheme: light)").matches
  ) {
    return "light";
  }
  return "dark";
}

/** Apply a theme class to the <html> element. */
function applyThemeClass(resolved: "dark" | "light" | "high-contrast") {
  const root = document.documentElement;
  for (const cls of THEME_CLASSES) {
    root.classList.remove(cls);
  }
  root.classList.add(resolved);
}

/** Initialize the theme from the stored setting. */
export async function initTheme(): Promise<void> {
  let theme: Theme = "dark";
  try {
    const raw = await invoke<string>("get_setting", { key: "theme" });
    const parsed = JSON.parse(raw) as string;
    if (["dark", "light", "high-contrast", "auto"].includes(parsed)) {
      theme = parsed as Theme;
    }
  } catch {
    // Use default
  }
  setTheme(theme);
}

const LISTENER_KEY = "__echotype_theme_listener";

/** Remove any existing OS preference listeners. */
function removeAutoListeners(): void {
  const existing = (globalThis as Record<string, unknown>)[LISTENER_KEY] as
    | (() => void)
    | undefined;
  if (existing && globalThis.matchMedia) {
    globalThis
      .matchMedia("(prefers-color-scheme: dark)")
      .removeEventListener("change", existing);
    globalThis
      .matchMedia("(prefers-contrast: more)")
      .removeEventListener("change", existing);
    delete (globalThis as Record<string, unknown>)[LISTENER_KEY];
  }
}

/** Set the active theme and persist it. */
export function setTheme(theme: Theme): void {
  const resolved = theme === "auto" ? resolveAuto() : theme;
  applyThemeClass(resolved);

  // Always clean up existing listeners first
  removeAutoListeners();

  // Listen for OS preference changes when in auto mode
  if (theme === "auto" && globalThis.matchMedia) {
    const handler = () => {
      const newResolved = resolveAuto();
      applyThemeClass(newResolved);
    };
    globalThis
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", handler);
    globalThis
      .matchMedia("(prefers-contrast: more)")
      .addEventListener("change", handler);
    (globalThis as Record<string, unknown>)[LISTENER_KEY] = handler;
  }
}
