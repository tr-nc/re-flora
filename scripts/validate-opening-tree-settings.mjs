#!/usr/bin/env node
// Real startup and save/load regression; no GPU work in cargo test.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const help = `Usage: node scripts/validate-opening-tree-settings.mjs
Requires cargo build --release and a Vulkan display. Runs hidden/muted fresh
openings with saved density/scale/maturity variants, then reloads a saved world
under different settings. Uses an isolated RE_FLORA_ROOT; never edits your config.
Logs/snapshots: target/opening-tree-settings/run-*/. Exit: 0 pass, 1 failure.`;
if (process.argv.includes('--help')) { console.log(help); process.exit(0); }
assert.equal(process.argv.length, 2, help);
const parent = path.join(root, 'target/opening-tree-settings');
fs.mkdirSync(parent, {recursive: true});
const output = fs.mkdtempSync(path.join(parent, 'run-'));
const runtime = path.join(output, 'runtime');
fs.mkdirSync(runtime);
fs.cpSync(path.join(root, 'config'), path.join(runtime, 'config'), {recursive: true});
fs.symlinkSync(path.join(root, 'assets'), path.join(runtime, 'assets'), 'dir');
const original = fs.readFileSync(path.join(root, 'config/gui.toml'), 'utf8');
const hash = () => createHash('sha256').update(fs.readFileSync(path.join(root, 'config/gui.toml'))).digest('hex');
const before = hash();
const env = {...process.env, RUST_LOG: 'info'};
for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
if (env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
env.RE_FLORA_ROOT = runtime;

function settings(scale, density, mature) {
    let text = original;
    for (const [id, value] of Object.entries({tree_planting_scale: scale,
        tree_planting_apple_density: density, tree_planting_instant_mature: mature})) {
        const pattern = new RegExp(`(id = "${id}"[\\s\\S]*?\\[section.param.data\\]\\s*value = )[^\\n]+`);
        assert.match(text, pattern);
        text = text.replace(pattern, (_, prefix) => prefix + String(value));
    }
    fs.writeFileSync(path.join(runtime, 'config/gui.toml'), text);
}
function run(name, extra) {
    const result = spawnSync(path.join(root, 'target/release/re-flora'),
        ['--hidden', '--mute', '--windowed', '--auto-exit', '0.5', ...extra],
        {cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
    const log = (result.stdout ?? '') + (result.stderr ?? '');
    fs.writeFileSync(path.join(output, name + '.log'), log);
    assert.equal(result.status, 0, `${name}: ${result.error ?? 'app failed'}; inspect ${output}`);
    assert.doesNotMatch(log, /\bERROR\b|VUID|panicked at|skipped_tree=/);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/);
    assert.equal(hash(), before, 'user config changed');
    return log;
}
// Read the version-2 raw snapshot's JSON garden footer, not compressed voxel data.
// The native reader validates the full file during the reload case below.
function trees(file) {
    const bytes = fs.readFileSync(file);
    assert.equal(bytes.subarray(0, 8).toString('hex'), '52464c54524e0001');
    assert.equal(bytes.readUInt32LE(8), 2);
    const count = bytes.readUInt32LE(48);
    const chunkBytes = [28, 32, 36, 40].reduce((n, offset) => n * bytes.readUInt32LE(offset), 1);
    const footer = bytes.readUInt32LE(12) + count * (bytes.readUInt32LE(52) + chunkBytes);
    const length = Number(bytes.readBigUInt64LE(footer));
    assert.equal(footer + 12 + length, bytes.length);
    return JSON.parse(bytes.subarray(footer + 12).toString('utf8')).trees.trees;
}
try {
    const variants = [
        {name: 'saved-density', scale: 1, density: 0.1, mature: true},
        {name: 'zero-half', scale: 0.5, density: 0, mature: true},
        {name: 'dense-double', scale: 2, density: 1, mature: true},
        {name: 'saplings', scale: 1.5, density: 0.2, mature: false},
    ];
    let saved;
    for (const {name, scale, density, mature} of variants) {
        settings(scale, density, mature);
        const file = path.join(output, name + '.rflterrain');
        const log = run(name, ['--terrain-save', file]);
        assert.equal((log.match(/\[GARDEN_OPENING\] planted_tree=/g) ?? []).length, 4);
        const captured = trees(file);
        assert.equal(captured.length, 4);
        for (const tree of captured) {
            assert.equal(tree.mature_desc.species, 'apple_tree');
            assert.ok(Math.abs(tree.mature_desc.size - 5.8 * 1.1 * scale) < 0.00001);
            assert.ok(Math.abs(tree.mature_desc.fruit_spawn_probability - density) < 0.000001);
            assert.equal(tree.age, mature ? 1 : 0.375);
            assert.equal(tree.growth_seconds, mature ? null : 0);
        }
        console.log(`${name}: four trees captured scale=${scale}, density=${density}, mature=${mature}`);
        if (name === 'saplings') saved = {file, captured};
    }
    settings(0.5, 0, true);
    const restoredFile = path.join(output, 'restored.rflterrain');
    const log = run('restore', ['--terrain-load', saved.file, '--terrain-save', restoredFile]);
    assert.doesNotMatch(log, /\[GARDEN_OPENING\] tree=/);
    assert.deepEqual(trees(restoredFile), saved.captured, 'loaded trees must retain their captured recipe and clock');
    console.log(`PASS: fresh settings and unchanged restore; evidence: ${output}`);
} finally {
    assert.equal(hash(), before, 'user config changed');
    fs.rmSync(runtime, {recursive: true, force: true});
}
