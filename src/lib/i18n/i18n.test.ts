import { describe, it, expect } from "vitest";
import { t } from "./index.js";

describe("i18n", () => {
  it("resolves known keys", () => {
    expect(t("app.title")).toBe("EchoType");
    expect(t("app.description")).toBe(
      "Local-first dictation for your desktop.",
    );
  });

  it("returns the key for unknown keys", () => {
    // @ts-expect-error testing unknown key behavior
    const result = t("unknown.key");
    expect(result).toBe("unknown.key");
  });

  it("interpolates parameters", () => {
    // "update.available" = "Update available: v{version}"
    expect(t("update.available", { version: "1.2.3" })).toBe(
      "Update available: v1.2.3",
    );
  });

  it("leaves unknown placeholders intact", () => {
    expect(t("update.available", {})).toBe("Update available: v{version}");
  });
});
