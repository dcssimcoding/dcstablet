// Real browser + test-hook executable. Synthetic source only; no DCS/display changes.
const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { chromium } = require(process.env.DCS_TABLET_PLAYWRIGHT || "playwright");
const root = path.resolve(__dirname, "..");
const executable =
  process.env.DCS_TABLET_FAULT_EXE ||
  path.join(root, "target", "debug", "dcs-tablet.exe");
const directory = fs.mkdtempSync(path.join(os.tmpdir(), "dcs-tablet-fault-"));
const fault = path.join(directory, "fault.txt");
const base = "http://127.0.0.1:18195";
const child = spawn(
  executable,
  ["--demo", "--port", "18195", "--test-fault-file", fault],
  { stdio: ["ignore", "pipe", "pipe"] },
);
let output = "";
child.stdout.on("data", (b) => (output += b));
child.stderr.on("data", (b) => (output += b));
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function status() {
  return (
    await fetch(base + "/status", { signal: AbortSignal.timeout(1500) })
  ).json();
}
async function until(check, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await check()) return;
    await delay(100);
  }
  throw Error("Condition did not recover in time");
}
let browser;
(async () => {
  try {
    await until(async () => {
      try {
        return (await status()).test_hooks === true;
      } catch {
        return false;
      }
    });
    browser = await chromium.launch({ headless: true, channel: process.env.DCS_TABLET_BROWSER_CHANNEL || undefined });
    const page = await browser.newPage({
      viewport: { width: 1180, height: 820 },
    });
    await page.goto(base);
    assert.ok(await page.evaluate(()=>RTCRtpReceiver.getCapabilities("video").codecs.some(codec=>codec.mimeType.toLowerCase()==="video/h264")),"Browser lacks H.264; use DCS_TABLET_BROWSER_CHANNEL=chrome or msedge.");
    async function connect() {
      await page.locator("#connect").click();
      await page.waitForSelector("#stage.playing", { timeout: 25000 });
      await page.waitForFunction(
        () =>
          document.querySelector("#source-video").getVideoPlaybackQuality()
            .totalVideoFrames > 2,
      );
    }
    await connect();
    for (const name of [
      "encoder-error",
      "encoder-panic",
      "capture-exit",
      "capture-panic",
    ]) {
      fs.writeFileSync(fault, name);
      await until(async () => {
        const s = await status();
        return s.active_viewers === 0 && !!s.last_error;
      });
      await page.waitForFunction(
        () => !document.querySelector("#connect").disabled,
      );
      await connect();
      assert.equal(
        (await status()).last_error,
        null,
        "new session clears the previous root cause",
      );
      console.log("PASS failure cleanup and reconnect:", name);
    }
    const before = await (await fetch(base + "/metrics")).json();
    const frames = await page
      .locator("#source-video")
      .evaluate((v) => v.getVideoPlaybackQuality().totalVideoFrames);
    fs.writeFileSync(fault, "sender-stall");
    await until(async () => !fs.existsSync(fault));
    await delay(1800);
    const after = await (await fetch(base + "/metrics")).json();
    assert.ok(
      after.encoded_frames_skipped > before.encoded_frames_skipped,
      "sender delay must exercise encoded FIFO overflow",
    );
    assert.ok(
      (await page
        .locator("#source-video")
        .evaluate((v) => v.getVideoPlaybackQuality().totalVideoFrames)) >
        frames + 10,
    );
    const colors = await page
      .locator(".screen canvas")
      .evaluateAll((cs) =>
        cs.map((c) => [...c.getContext("2d").getImageData(64, 64, 1, 1).data]),
      );
    assert.ok(colors[0][1] > 100 && colors[0][0] < 50, JSON.stringify(colors));
    assert.ok(colors[1][2] > 100 && colors[1][1] < 100, JSON.stringify(colors));
    console.log("PASS bounded encoded overflow and clean video recovery");
    fs.writeFileSync(fault, "capture-stall");
    await until(async () => (await status()).capture_source.state === "stale");
    await page
      .locator("#source-status")
      .filter({ hasText: "not updated" })
      .waitFor();
    assert.equal(
      (await status()).active_viewers,
      1,
      "stale source must not force-disconnect static content",
    );
    await until(async () => (await status()).capture_source.state === "fresh");
    await until(
      async () => (await page.locator("#source-status").innerText()) === "",
    );
    await page.locator("#disconnect").click();
    await until(async () => (await status()).active_viewers === 0);
    console.log("PASS source freshness warning, recovery, and viewer release");
  } finally {
    await browser?.close();
    try {
      await fetch(base + "/shutdown", { method: "POST" });
    } catch {}
    for (let i = 0; i < 50 && child.exitCode === null; i++) await delay(100);
    if (child.exitCode === null) child.kill();
    fs.rmSync(directory, { recursive: true, force: true });
  }
})().catch((error) => {
  console.error(error);
  console.error(output);
  process.exitCode = 1;
});
