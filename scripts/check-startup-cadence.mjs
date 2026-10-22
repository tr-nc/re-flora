#!/usr/bin/env node
import fs from 'node:fs';
import {pathToFileURL} from 'node:url';

const HELP = `Usage: node scripts/check-startup-cadence.mjs <run.log> [--min-fps N] [--max-record-ms N]

Check ordinary startup cadence from a completed Release --perf run.
Print JSON; optional positive budgets are host-specific, not universal defaults.
Discard the first five seconds of CPU scope samples; require another five seconds.
FPS uses frame-number/time differences, not inverse GPU timestamps. Means use
logged CPU samples (ordinary --perf logs every 30 frames). Reject errors, missing
shutdown, nonmonotonic frames, or changing render dimensions. Retain/report WARNs.

Capture without changing saved quality or present mode, e.g.:
  RUST_LOG=info target/release/re-flora --hidden --mute --perf --auto-exit 16 > target/startup-cadence.log 2>&1
  node scripts/check-startup-cadence.mjs target/startup-cadence.log --min-fps 50 --max-record-ms 8
Do not use GLIBC_TUNABLES overrides when checking the mapped-upload repair.
Run comparisons serially on the same machine, scene, settings and dimensions.
Exit: 0 passed/statistics/help; 1 evidence or budget failure; 2 invalid arguments.
`;
const requireThat = (condition, message) => { if (!condition) throw Error(message); };

export function analyze(text) {
  requireThat(!/\bERROR\b|panicked at|VUID-|SYNC-HAZARD/.test(text), 'run contains an error; inspect the original log before repeating');
  requireThat(text.includes('[SHUTDOWN] phase=complete failures=0'), 'missing clean shutdown; capture a complete run');
  const dimensions = [...text.matchAll(/\[SCENE_PIXELS\] screen=(\d+x\d+) scene=(\d+x\d+)/g)].map(m => ({screen:m[1],scene:m[2]}));
  requireThat(dimensions.length && dimensions.every(d => d.screen === dimensions[0].screen && d.scene === dimensions[0].scene), 'missing or changing dimensions; use a steady ordinary startup run, not a resize test');
  let day = 0, previousClock = -1;
  const rows = [...text.matchAll(/\[(\d+):(\d+):(\d+\.\d+) [^\n]*?\[PERF\]\[CPU_FRAME_SCOPE\] frame (\d+) ([^\n]+)/g)].map(m => {
    const clock = +m[1]*3600 + +m[2]*60 + +m[3];
    if (clock < previousClock) { requireThat(previousClock-clock > 43200, 'CPU timestamps went backwards'); day += 86400; }
    previousClock = clock;
    const record = m[5].match(/\brender\.record=(\d+)us/);
    requireThat(record, 'CPU scope lacks render.record; use a compatible --perf log');
    return {time:day+clock,frame:+m[4],recordMs:+record[1]/1000};
  });
  requireThat(rows.every((r,i) => i === 0 || (r.frame > rows[i-1].frame && r.time > rows[i-1].time)), 'CPU frames/timestamps must increase');
  const warm = rows.filter(r => r.time >= rows[0].time + 5);
  requireThat(warm.length >= 2 && warm.at(-1).time-warm[0].time >= 5, 'not enough warmed samples; rerun the capture with --auto-exit 22');
  const seconds = warm.at(-1).time-warm[0].time;
  return {...dimensions[0],samples:warm.length,seconds,
    fps:(warm.at(-1).frame-warm[0].frame)/seconds,
    recordMeanMs:warm.reduce((sum,r) => sum+r.recordMs,0)/warm.length,
    warnings:text.split('\n').filter(l => /\bWARN\b/.test(l)).length};
}

export function options(args) {
  if (args.length === 1 && ['--help','-h'].includes(args[0])) return {help:true};
  requireThat(args.length && !args[0].startsWith('-'), 'expected a run.log path');
  const result = {file:args[0]};
  for (let i=1;i<args.length;i+=2) {
    const key = {'--min-fps':'minFps','--max-record-ms':'maxRecordMs'}[args[i]];
    requireThat(key, `unknown option: ${args[i]}`);
    requireThat(!(key in result), `duplicate option: ${args[i]}`);
    const value = Number(args[i+1]);
    requireThat(Number.isFinite(value) && value > 0, `${args[i]} requires a positive finite number`);
    result[key] = value;
  }
  return result;
}

export function checkBudgets(result, opts) {
  requireThat(opts.minFps === undefined || result.fps >= opts.minFps, `cadence ${result.fps.toFixed(2)} FPS < ${opts.minFps}; inspect CPU/GPU scopes, do not mask the regression by lowering quality`);
  requireThat(opts.maxRecordMs === undefined || result.recordMeanMs <= opts.maxRecordMs, `CPU recording ${result.recordMeanMs.toFixed(3)} ms > ${opts.maxRecordMs}; inspect mapped uploads and other recording work`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let opts;
  try { opts = options(process.argv.slice(2)); }
  catch (error) { console.error(`${error.message}\n\n${HELP}`); process.exitCode = 2; }
  if (opts?.help) console.log(HELP);
  else if (opts) {
    try {
      const result = analyze(fs.readFileSync(opts.file,'utf8'));
      console.log(JSON.stringify(result,null,2));
      if (result.warnings) console.error(`${result.warnings} WARN lines retained; inspect ${opts.file}; cadence success is not full correctness acceptance`);
      checkBudgets(result,opts);
    } catch (error) { console.error(`${error.message}\nInspect ${opts.file}; for capture/retry syntax use --help.`); process.exitCode = 1; }
  }
}
