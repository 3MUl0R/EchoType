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
});
