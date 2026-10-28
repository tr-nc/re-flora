#!/usr/bin/env node
// Pixel regression through the native Vulkan compositor, not a CPU renderer mock.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync, execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';

const help = `Usage: node scripts/validate-matte-stack.mjs
Requires cargo build --release, a Vulkan display and ImageMagick (magick).
Captures three overlapping glass panels in hidden/muted Release mode, changes
the bottom panel's color, reverses paint order, fades and clips the upper panel.
Verifies transmission and increasing attenuation through one and two glass layers,
not just successful startup.
Artifacts: target/matte-stack/run-*/. No saved settings are changed.
Exit: 0 pass, 1 runtime/pixel failure.`;
if (process.argv.includes('--help')) { console.log(help); process.exit(0); }
assert.equal(process.argv.length, 2, help);
const root = path.resolve(import.meta.dirname, '..');
const parent = path.join(root, 'target/matte-stack');
fs.mkdirSync(parent, {recursive: true});
const output = fs.mkdtempSync(path.join(parent, 'run-'));
const configHash = () => createHash('sha256').update(fs.readFileSync(path.join(root, 'config/gui.toml'))).digest('hex');
const before = configHash();
const env = {...process.env, RUST_LOG: 'info'};
for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
if (env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
const means = {};
try {
    for (const mode of ['red', 'blue', 'red-reverse', 'blue-reverse', 'red-fade', 'blue-fade', 'red-clip', 'blue-clip']) {
        const image = path.join(output, mode + '.png');
        const run = spawnSync(path.join(root, 'target/release/re-flora'), [
            '--hidden', '--mute', '--windowed', '--screenshot', 'player-default', image,
            '--screenshot-delay', '1', '--auto-exit', '2',
        ], {cwd: root, env: {...env, RE_FLORA_MATTE_STACK_REVIEW: mode},
            encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
        const log = (run.stdout ?? '') + (run.stderr ?? '');
        fs.writeFileSync(path.join(output, mode + '.log'), log);
        assert.equal(run.status, 0, `${mode}: ${run.error ?? 'app failed'}; inspect ${output}`);
        assert.doesNotMatch(log, /\bERROR\b|VUID|SYNC-HAZARD|panicked at|Callback primitives not yet supported/);
        assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/);
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
    fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({means, response, reversed, faded, clipped}, null, 2));
    console.log(`PASS: bottom-color response across 0/1/2 glass layers = ${response.join(', ')}; reversed = ${reversed}`);
    console.log(`Evidence: ${output}`);
} finally {
    assert.equal(configHash(), before, 'saved config changed');
}
