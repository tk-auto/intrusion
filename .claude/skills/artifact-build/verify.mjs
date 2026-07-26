// Smoke-verify the assembled artifact page in headless Chromium before it is
// published: the game must boot with no page errors, paint a non-blank canvas,
// and respond to arrow-key input (the frame after pressing the arrows must
// differ from the boot frame — the player always spawns with at least one open
// neighbour, so an identical frame means input is broken).
//
// A replay build (#197) is checked differently: the arrows scrub a time cursor,
// not the player, so we assert the replay HUD reads `0 / N` at boot, advances on
// ArrowRight, and rewinds back to `0` on ArrowLeft — the scrub loop actually works.
//
// Usage: node verify.mjs <assembled.html> <shots-dir>
// Env overrides: PLAYWRIGHT_MODULE (index.mjs path), CHROMIUM_PATH.

import { mkdirSync, readFileSync, writeFileSync } from "fs";
import { execSync } from "child_process";
import { resolve } from "path";

const [htmlPath, shotsDir] = process.argv.slice(2);
if (!htmlPath || !shotsDir) {
  console.error("usage: node verify.mjs <assembled.html> <shots-dir>");
  process.exit(2);
}

const pwModule =
  process.env.PLAYWRIGHT_MODULE ??
  resolve(execSync("npm root -g").toString().trim(), "playwright/index.mjs");
const { chromium } = await import(pwModule);

mkdirSync(shotsDir, { recursive: true });

// Reproduce the Artifact host's wrapping: the assembled file is body content.
const wrapped = resolve(shotsDir, "wrapped.html");
writeFileSync(
  wrapped,
  "<!doctype html><html><head></head><body>" +
    readFileSync(htmlPath, "utf8") +
    "</body></html>",
);

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH ?? "/opt/pw-browsers/chromium",
});
const page = await browser.newPage({ viewport: { width: 900, height: 600 } });

const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => {
  if (m.type() === "error") errors.push(m.text());
});

await page.goto("file://" + wrapped);
await page.waitForTimeout(1500);

const canvas = await page.$("canvas");
if (!canvas) {
  console.error("verify: FAIL — no canvas mounted");
  await browser.close();
  process.exit(1);
}
// An unbaked live build opens on the title screen (#268), where the arrows would
// only move a selection. Screenshot the menu, then press Enter on Quick play so the
// checks below exercise the real game. A build with a baked seed (or a replay) is
// already in its run and reports no menu, so this is skipped.
const screen = () => page.evaluate(() => document.body.dataset.screen ?? "");
if ((await screen()) === "menu") {
  await page.screenshot({ path: resolve(shotsDir, "menu.png") });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(500);
  const now = await screen();
  if (now !== "play") {
    console.error(`verify: FAIL — Quick play did not start a run (screen "${now}")`);
    await browser.close();
    process.exit(1);
  }
}

const boot = await page.screenshot({ path: resolve(shotsDir, "boot.png") });

// A blank canvas screenshots as a near-empty PNG; the glyph grid does not.
if (boot.length < 5000) {
  console.error(`verify: FAIL — boot frame suspiciously empty (${boot.length}B png)`);
  await browser.close();
  process.exit(1);
}

// A replay build reveals a #replaybar HUD; a live build never does. Branch on it.
const replayPos = await page.$("#replaybar.on #replay-pos");
let moved = false;
if (replayPos) {
  // Replay viewer (#197): the arrows scrub the time cursor, not the player. Assert
  // the HUD starts at `0 / N`, advances on ArrowRight, and rewinds back on ArrowLeft.
  const read = () => page.$eval("#replay-pos", (el) => el.textContent.trim());
  const at0 = await read();
  if (!/^0 \/ \d+$/.test(at0)) {
    console.error(`verify: FAIL — replay HUD did not boot at "0 / N" (got "${at0}")`);
    await browser.close();
    process.exit(1);
  }
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(100);
  const at1 = await read();
  await page.screenshot({ path: resolve(shotsDir, "after-input.png") });
  await page.keyboard.press("ArrowLeft");
  await page.waitForTimeout(100);
  const back = await read();
  if (at1 !== at0 && back === at0) {
    moved = true;
  } else {
    console.error(
      `verify: FAIL — replay scrub broken (0:"${at0}" → right:"${at1}" → left:"${back}")`,
    );
    await browser.close();
    process.exit(1);
  }
} else {
  // Live build: try one direction at a time (a full right-down-left-up sweep can
  // cancel out and land the player back on the boot square); pass on the first change.
  for (const key of ["ArrowRight", "ArrowDown", "ArrowLeft", "ArrowUp"]) {
    await page.keyboard.press(key);
    await page.waitForTimeout(100);
    const after = await page.screenshot({
      path: resolve(shotsDir, "after-input.png"),
    });
    if (Buffer.compare(boot, after) !== 0) {
      moved = true;
      break;
    }
  }
}
await browser.close();

if (errors.length) {
  console.error("verify: FAIL — page errors:\n" + errors.join("\n"));
  process.exit(1);
}
if (!moved) {
  console.error("verify: FAIL — frame unchanged after arrow input");
  process.exit(1);
}
console.log(`verify: OK — boot ${boot.length}B, frame changed after input; ` +
  `screenshots in ${shotsDir} (now Read them)`);
