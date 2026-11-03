const fs = require('node:fs');
const median = values => {
  const sorted = [...values].sort((a, b) => a - b), middle = Math.floor(sorted.length / 2);
  return sorted.length ? (sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2) : null;
};
function time(line) {
  const m = line.match(/^\[(\d\d):(\d\d):(\d\d)\.(\d\d\d)/);
  return m ? ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +m[4] : NaN;
}
const result = {};
for (const mode of ['off', 'on']) {
  const lines = fs.readFileSync(`target/source-vis/final-${mode}.log`, 'utf8').split('\n');
  const frames = lines.flatMap(line => {
    const match = line.match(/\[PERF\]\[FRAME\] frame (\d+) total ([\d.]+)ms/);
    return match ? [{ n: +match[1], ms: time(line) }] : [];
  });
  const end = frames.at(-1).ms, start = end - 6000, tail = frames.filter(f => f.ms >= start);
  const first = tail[0], last = tail.at(-1);
  const allScopes = lines.filter(l => l.includes('[GPU_FRAME_SCOPE]'));
  const scopes = allScopes.filter(l => time(l) >= start && time(l) <= end);
  const values = (list, key) => list.flatMap(line => {
    const match = line.match(new RegExp(key.replaceAll('.', '\\.') + '=(\\d+)us'));
    return match ? [+match[1] / 1000] : [];
  });
  const gpu = {};
  for (const key of ['frame.render', 'graphics.flora_lighting_cache', 'graphics.local_light_field', 'ddgi.probe_trace']) {
    const samples = values(scopes, key);
    gpu[key] = { samples: samples.length, medianMs: median(samples) };
  }
  const builds = values(allScopes, 'graphics.local_light_field');
  const began = lines.findLast(l => l.includes('ready_recorded=false'));
  const ready = lines.findLast(l => l.includes('ready_recorded=true'));
  result[mode] = {
    elapsedMs: last.ms - first.ms, frameAdvancement: last.n - first.n,
    actualFps: 1000 * (last.n - first.n) / (last.ms - first.ms), gpu,
    build: { samples: builds.length, medianMs: median(builds), maxSampleMs: builds.length ? Math.max(...builds) : null,
      stableKeyBuildElapsedMs: ready && began ? time(ready) - time(began) : null },
  };
}
console.log(JSON.stringify(result, null, 2));
