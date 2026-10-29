#!/usr/bin/env node
// Pixel regression through the native Vulkan compositor, not a CPU renderer mock.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync, execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';

const help = `Usage: node scripts/validate-matte-stack.mjs [--pixelated]
Requires cargo build --release, a Vulkan display and ImageMagick (magick).
Captures three overlapping glass panels in hidden/muted Release mode, changes
bottom color, reverses paint order, fades and clips the upper panel.
Pixelated glass is always enabled; --pixelated is accepted for older invocations.
Also verifies cell uniformity and five-tap averages of a patterned lower panel
through independently anchored panels at sizes 12 and 21, including panel movement,
clipping (which must not reset grid phase), and viewport resize.
Uses an isolated config; never changes your saved settings.
Artifacts: target/matte-stack/run-*/. Exit: 0 pass/help, 1 failure, 2 usage error.`;
const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--help') { console.log(help); process.exit(0); }
if (args.length > 1 || (args.length && args[0] !== '--pixelated')) {
    console.error(`Unknown arguments: ${args.join(' ')}\n${help}`); process.exit(2);
}
const pixelated = true;
const root = path.resolve(import.meta.dirname, '..');
const parent = path.join(root, 'target/matte-stack');
fs.mkdirSync(parent, {recursive: true});
const output = fs.mkdtempSync(path.join(parent, 'run-'));
const runtime = path.join(output, 'runtime');
fs.mkdirSync(runtime);
fs.cpSync(path.join(root, 'config'), path.join(runtime, 'config'), {recursive: true});
fs.symlinkSync(path.join(root, 'assets'), path.join(runtime, 'assets'), 'dir');
const original = fs.readFileSync(path.join(root, 'config/gui.toml'), 'utf8');
const configHash = () => createHash('sha256').update(fs.readFileSync(path.join(root, 'config/gui.toml'))).digest('hex');
const before = configHash();
const env = {...process.env, RUST_LOG: 'info'};
for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
if (env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
env.RE_FLORA_ROOT = runtime;
const means = {};
function settings(cellSize) {
    let text = original;
    for (const [id, value] of Object.entries({ui_glass_cell_size: cellSize})) {
        const pattern = new RegExp(`(id = "${id}"[\\s\\S]*?\\[section.param.data\\]\\s*value = )[^\\n]+`);
        assert.match(text, pattern);
        text = text.replace(pattern, (_, prefix) => prefix + String(value));
    }
    fs.writeFileSync(path.join(runtime, 'config/gui.toml'), text);
}
function capture(mode, name = mode, extra = []) {
    const image = path.join(output, name + '.png');
    const run = spawnSync(path.join(root, 'target/release/re-flora'), [
        '--hidden', '--mute', '--windowed', '--screenshot', 'player-default', image,
        '--screenshot-delay', '1', '--auto-exit', '2', ...extra,
    ], {cwd: root, env: {...env, RE_FLORA_MATTE_STACK_REVIEW: mode},
        encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
    const log = (run.stdout ?? '') + (run.stderr ?? '');
    fs.writeFileSync(path.join(output, name + '.log'), log);
    assert.equal(run.status, 0, `${name}: ${run.error ?? 'app failed'}; inspect ${output}`);
    assert.doesNotMatch(log, /\bERROR\b|VUID|SYNC-HAZARD|panicked at|Callback primitives not yet supported/);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/);
    return image;
}
function gridCheck(image, size, moved = false) {
    const width = Number(execFileSync('magick', [image, '-format', '%w', 'info:'], {encoding: 'utf8'}));
    const bytes = execFileSync('magick', [image, '-depth', '8', 'rgb:-'], {maxBuffer: 32 * 1024 * 1024});
    const tint = rgb => {
        const luminance = rgb[0] * 0.213 + rgb[1] * 0.715 + rgb[2] * 0.072;
        return rgb.map((v, c) => (v * 0.5 + luminance * 0.5) * 0.82 + [217, 228, 194][c] / 255 * 0.18);
    };
    const origins = [null, moved ? [107, 105] : [100, 100], [180, 160]];
    const cellStart = (x, y, layer) => origins[layer].map((o, a) =>
        o + Math.floor(([x, y][a] - o) / size) * size);
    const cache = new Map();
    function expected(x, y, layer) {
        if (layer === 0) {
            const tx = Math.floor((x - 64) / 4), ty = Math.floor((y - 64) / 4);
            return [(tx * 17 + ty * 29) % 251, (tx * 43 + ty * 7) % 251, (tx * 3 + ty * 61) % 251].map(v => v / 255);
        }
        const [x0, y0] = cellStart(x, y, layer), key = [layer, x0, y0].join(',');
        if (cache.has(key)) return cache.get(key);
        let average = [0, 0, 0];
        // Independent five-point oracle: center + quadrant centers, nearest
        // texels, equal weights. These interior fixtures do not touch viewport edges.
        for (const [u, v] of [[0.5, 0.5], [0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
            const rgb = expected(x0 + Math.floor(size * u), y0 + Math.floor(size * v), layer - 1);
            average = average.map((value, c) => value + rgb[c] / 5);
        }
        const result = tint(average);
        cache.set(key, result);
        return result;
    }
    let worstError = 0;
    for (const [sx, sy, layers] of [[145, 150, 1], [300, 210, 2]]) {
        const [x0, y0] = cellStart(sx, sy, layers);
        const average = expected(sx, sy, layers);
        const first = [...bytes.subarray((y0 * width + x0) * 3, (y0 * width + x0) * 3 + 3)];
        for (let y = y0; y < y0 + size; y++) for (let x = x0; x < x0 + size; x++) {
            const rgb = [...bytes.subarray((y * width + x) * 3, (y * width + x) * 3 + 3)];
            assert.deepEqual(rgb, first, `${size}px cell must be exactly uniform at ${x},${y}`);
            for (let c = 0; c < 3; c++) worstError = Math.max(worstError, Math.abs(rgb[c] - average[c] * 255));
        }
    }
    assert.ok(worstError <= 2, `five-tap average differs by ${worstError} bytes; inspect ${image}`);
    return {size, moved, worstError};
}
try {
    settings(12);
    for (const mode of ['red', 'blue', 'red-reverse', 'blue-reverse', 'red-fade', 'blue-fade', 'red-clip', 'blue-clip']) {
        const image = capture(mode);
        means[mode] = ['20x20+70+70', '30x20+120+130', '30x30+300+200'].map(crop =>
            Number(execFileSync('magick', [image, '-crop', crop, '-format', '%[fx:mean.r]', 'info:'], {encoding: 'utf8'})));
    }
    const response = means.red.map((red, i) => red - means.blue[i]);
    assert.ok(response[0] > 0.7, 'fixture must expose an unfiltered bottom-panel color');
    assert.ok(response[1] > 0.2, 'bottom contents disappeared under one glass layer');
    assert.ok(response[2] > 0.1, 'bottom contents disappeared under two glass layers');
    assert.ok(response[0] > response[1] + 0.1 && response[1] > response[2] + 0.08,
        'glass tint/desaturation must accumulate with each layer');
    const reversed = means['red-reverse'][2] - means['blue-reverse'][2];
    assert.ok(reversed > 0.7, 'reversing paint order must place the colored panel on top');
    const faded = means['red-fade'][2] - means['blue-fade'][2];
    assert.ok(faded > response[2] + 0.03 && faded < response[1] - 0.03,
        'half-faded upper glass must mix with the lower composite');
    const clipped = means['red-clip'][2] - means['blue-clip'][2];
    assert.ok(Math.abs(clipped - response[1]) < 0.015,
        'outside the upper clip rectangle, the lower panel must remain unchanged');
    const grids = [];
    for (const size of [12, 21]) {
        settings(size);
        for (const mode of ['grid', 'grid-moved', 'grid-moved-clip']) {
            grids.push(gridCheck(capture(mode, `${mode}-${size}`, size === 21 && mode === 'grid' ? ['--resize-lifecycle-test'] : []), size, mode !== 'grid'));
        }
    }
    fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({pixelated, means, response, reversed, faded, clipped, grids}, null, 2));
    console.log(`PASS: pixelated bottom-color response 0/1/2 layers = ${response.join(', ')}; grids = ${JSON.stringify(grids)}`);
    console.log(`Evidence: ${output}`);
} finally {
    assert.equal(configHash(), before, 'saved config changed');
    fs.rmSync(runtime, {recursive: true, force: true});
}
