#!/usr/bin/env node
// Matched Release timings for native UI glass; not a cargo test or FPS promise.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {createHash} from 'node:crypto';
const help = `Usage: node scripts/benchmark-ui-glass.mjs --before PATH --after PATH [--seconds 8] [--repeats 3]
Compare two already-built Release binaries at the native fullscreen resolution,
with the same saved scene/quality and Debug Panel searching UI Glass. Runs original
Gaussian and pixelated glass, hidden/muted with --perf; alternates variant order.
Requires a Vulkan display. Stop other game/GPU jobs before running; do not build
concurrently. Uses isolated config copies, never writes your settings.
Default: 8 seconds after loading per case, 3 paired repeats (12 runs).
Retains the last 30 CPU frame samples and up to 3 GPU scope samples per run;
reports median frame, total GPU and egui GPU milliseconds. These short diagnostic
samples are not a general performance-budget acceptance or a VSync-free benchmark.
Evidence: target/matte-perf/paired-*/ including logs, hashes and summary.json.
Exit: 0 captured/help, 1 runtime/evidence failure, 2 invalid arguments.`;
let options;
try {
    options = parseArgs({options: {before: {type: 'string'}, after: {type: 'string'},
        seconds: {type: 'string', default: '8'}, repeats: {type: 'string', default: '3'},
        help: {type: 'boolean'}}}).values;
    if (options.help) { console.log(help); process.exit(0); }
    if (!options.before || !options.after || !Number.isFinite(+options.seconds) || +options.seconds < 8 ||
        !Number.isInteger(+options.repeats) || +options.repeats < 1) throw Error('Require both binaries, seconds >= 8 and integer repeats >= 1.');
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }
const root = path.resolve(import.meta.dirname, '..');
const parent = path.join(root, 'target/matte-perf');
fs.mkdirSync(parent, {recursive: true});
const output = fs.mkdtempSync(path.join(parent, 'paired-'));
const runtime = path.join(output, 'runtime');
fs.mkdirSync(runtime);
fs.cpSync(path.join(root, 'config'), path.join(runtime, 'config'), {recursive: true});
fs.symlinkSync(path.join(root, 'assets'), path.join(runtime, 'assets'), 'dir');
const hash = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const config = path.join(root, 'config/gui.toml'), initialHash = hash(config);
const original = fs.readFileSync(config, 'utf8');
const rows = [], binaries = {before: path.resolve(options.before), after: path.resolve(options.after)};
const binaryHashes = Object.fromEntries(Object.entries(binaries).map(([k, v]) => [k, hash(v)]));
const median = values => values.toSorted((a, b) => a - b)[Math.floor(values.length / 2)];
try {
    for (let repeat = 0; repeat < +options.repeats; repeat++) for (const pixelated of [false, true]) {
        const pattern = /(id = "ui_glass_pixelated"[\s\S]*?\[section.param.data\]\s*value = )[^\n]+/;
        assert.match(original, pattern);
        fs.writeFileSync(path.join(runtime, 'config/gui.toml'), original.replace(pattern, (_, p) => p + pixelated));
        for (const variant of repeat % 2 ? ['after', 'before'] : ['before', 'after']) {
            const name = `${repeat}-${pixelated ? 'pixelated' : 'gaussian'}-${variant}`;
            const env = {...process.env, RUST_LOG: 'info'};
            for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_') || key.startsWith('VK_LAYER')) delete env[key];
            delete env.VK_INSTANCE_LAYERS;
            if (env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
            Object.assign(env, {RE_FLORA_ROOT: runtime, RE_FLORA_DEBUG_PANEL_REVIEW: '1', RE_FLORA_DEBUG_SEARCH_REVIEW: 'UI Glass'});
            const run = spawnSync(binaries[variant], ['--hidden', '--mute', '--perf', '--auto-exit', options.seconds],
                {cwd: root, env, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024});
            const log = (run.stdout ?? '') + (run.stderr ?? '');
            fs.writeFileSync(path.join(output, name + '.log'), log);
            assert.equal(run.status, 0, `${name}: ${run.error ?? 'app failed'}`);
            assert.doesNotMatch(log, /\bERROR\b|VUID|SYNC-HAZARD|panicked at/);
            assert.match(log, /phase=complete failures=0/);
            const frame = [...log.matchAll(/\[PERF\]\[FRAME\] frame \d+ total ([\d.]+)ms/g)].map(m => +m[1]).slice(-30);
            const gpu = [...log.matchAll(/frame.render=(\d+)us/g)].map(m => +m[1] / 1000).slice(-3);
            const ui = [...log.matchAll(/egui.render=(\d+)us/g)].map(m => +m[1] / 1000).slice(-3);
            assert.ok(frame.length === 30 && gpu.length && ui.length, `${name}: insufficient steady timing samples`);
            const screen = log.match(/\[SCENE_PIXELS\] ([^\n]+)/)?.[1];
            assert.ok(screen);
            if (rows.length) assert.equal(screen, rows[0].screen, 'render dimensions/quality must match');
            const row = {name, variant, repeat, pixelated, screen, frame_ms: median(frame), gpu_ms: median(gpu), ui_ms: median(ui), samples: {frame, gpu, ui}};
            rows.push(row);
            fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({options, initialHash, binaryHashes, rows}, null, 2));
            console.log(JSON.stringify({name, frame_ms: row.frame_ms, gpu_ms: row.gpu_ms, ui_ms: row.ui_ms}));
            assert.equal(hash(config), initialHash, 'user config changed');
        }
    }
    console.log(`Evidence: ${output}`);
} catch (error) { console.error(`${error.stack}\nInspect ${output}; correct the failure before rerunning.`); process.exitCode = 1; }
finally { fs.rmSync(runtime, {recursive: true, force: true}); }
