#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const usage = "Usage: node scripts/check-startup-log.mjs <run.log>\nChecks a completed Release startup log captured with the default log filter.";
const args = process.argv.slice(2);
if (args.length === 1 && ["-h", "--help"].includes(args[0])) {
  console.log(usage);
  process.exit(0);
}
if (args.length !== 1 || args[0].startsWith("-")) {
  console.error(`Expected one run-log path.\n${usage}\nExample: node scripts/check-startup-log.mjs target/startup-review/default-smoke.log`);
  process.exit(2);
}

try {
  const log = readFileSync(args[0], "utf8");
  const selected = log.indexOf("Selected physical device:");
  const firstFrame = log.indexOf("Loading complete; first game frame ready");
  assert(selected >= 0 && firstFrame > selected, "missing device selection or first completed frame");
  assert(log.slice(selected, firstFrame).includes("[STARTUP]"), "no startup progress between GPU selection and first completed frame");
  const messages = [...log.matchAll(/\[STARTUP\] status=(\w+) stage=(\w+) stage_ms=([\d.]+) total_ms=([\d.]+)/g)];
  assert(messages.length > 0, "no structured startup messages");
  let active = null, ready = false, previousTime = 0, largestGap = 0, heartbeats = 0;
  const stages = {};
  for (const message of messages) {
    const [, status, stage, stageMs, totalMs] = message;
    const time = Number(totalMs);
    assert(!ready, "startup reporter continued after readiness");
    assert(time >= previousTime, "startup total time moved backwards");
    largestGap = Math.max(largestGap, time - previousTime);
    previousTime = time;
    assert(Number(stageMs) <= time, "stage elapsed time exceeds total startup time");
    if (status === "begin") {
      assert.equal(active, null, "stage started before its predecessor finished");
      active = stage;
    } else if (status === "waiting") {
      assert.equal(stage, active, "heartbeat names a stale stage");
      heartbeats++;
    } else if (status === "done") {
      assert.equal(stage, active, "completion names a stale stage");
      stages[stage] = Number(stageMs);
      active = null;
    } else if (status === "ready") {
      assert.equal(active, null);
      assert.equal(stage, "first_frame");
      assert(message.index > firstFrame, "readiness reported before first-frame completion");
      ready = true;
    } else {
      assert.fail(`startup did not finish normally: status=${status} stage=${stage}`);
    }
  }
  assert(ready, "missing startup readiness");
  for (const stage of ["shader_modules", "renderer_resources", "compute_pipelines", "graphics_pipelines", "terrain_and_model_caches", "first_frame"]) {
    assert(Object.hasOwn(stages, stage), `missing measured stage: ${stage}`);
  }
  // The reporter's 2s interval gets 1s scheduling allowance. This is a log
  // responsiveness check on the test host, not a startup-performance budget.
  assert(largestGap <= 3000, `silent startup gap of ${largestGap.toFixed(1)}ms exceeds 2s heartbeat + 1s allowance`);
  console.log(JSON.stringify({ totalMs: previousTime, largestGapMs: +largestGap.toFixed(1), heartbeats, stages }, null, 2));
} catch (error) {
  console.error(`Startup log check failed: ${error.message}\n${usage}`);
  process.exit(1);
}
