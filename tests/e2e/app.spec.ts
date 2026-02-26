import { test, expect } from "@playwright/test";

test("page loads and shows app title", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("h1")).toHaveText("EchoType");
});

test("page has navigation", async ({ page }) => {
  await page.goto("/");
  const nav = page.getByRole("navigation");
  await expect(nav).toBeVisible();
});

test("page has main content area", async ({ page }) => {
  await page.goto("/");
  const main = page.locator("main");
  await expect(main).toBeVisible();
});
