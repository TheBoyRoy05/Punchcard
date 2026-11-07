import { expect, test } from "@playwright/test";

import { at } from "../time";
import { HARNESS } from "./fixture";

const WIDTHS = [1200, 800, 520];
const SHOTS = ".screenshots";

for (const width of WIDTHS) {
  test(`renders the views at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 760 });
    await page.goto(HARNESS);
    await expect(page.locator("[data-key]").first()).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/day-${width}.png`, fullPage: true });

    await page.locator(`[data-key="Tasks/Serialize Hold.md|${at("11-02 15:00")}"]`).click();
    await page.screenshot({ path: `${SHOTS}/popup-${width}.png`, fullPage: true });

    await page.locator("[data-popup] [data-task]").click();
    await page.getByLabel("Move block to task").fill("o");
    await page.screenshot({ path: `${SHOTS}/picker-${width}.png`, fullPage: true });

    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "week" }).click();
    await page.getByRole("button", { name: "Previous" }).click();
    await expect(page.locator("[data-track]")).toHaveCount(7);
    await page.screenshot({ path: `${SHOTS}/week-${width}.png`, fullPage: true });
  });
}
