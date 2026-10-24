#!/usr/bin/env node
// Build a portable report. PNG originals are embedded once, unchanged; rebuilding can
// recover them from the existing HTML, so this bundle never requires a target/ directory.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = dirname(fileURLToPath(import.meta.url));
const output = resolve(directory, 'index.html');
const manifestPath = resolve(directory, 'capture-manifest.json');
const args = process.argv.slice(2);
if (args.includes('--help')) {
  console.log('Usage: node docs/reports/garden-tree-species/build.mjs [--captures DIRECTORY]\n\nWithout arguments, rebuilds from PNG originals embedded in index.html, verified\nagainst capture-manifest.json. With --captures, imports originals from DIRECTORY.\nWrites index.html and capture-manifest.json beside this script. No dependencies.');
  process.exit(0);
}
if (args.length && (args.length !== 2 || args[0] !== '--captures')) {
  throw new Error('Expected --captures DIRECTORY, or no arguments. Use --help.');
}
const records = [
  ['apple', 122, 250, 31329, 27], ['pine', 122, 460, 20098, 0], ['willow', 122, 448, 12605, 0],
  ['apple', 7, 250, 31402, 34], ['pine', 7, 428, 18628, 0], ['willow', 7, 448, 12580, 0],
  ['apple', 907, 250, 31569, 28], ['pine', 907, 476, 20906, 0], ['willow', 907, 448, 12650, 0],
];
const species = {
  apple: { label: '苹果树', english: 'Apple Tree', description: '低分叉与上扬骨干枝，错落重叠的圆润叶团。使用现有苹果生命周期，果实悬挂在树冠下方。' },
  pine: { label: '松树', english: 'Pine', description: '节间、枝数和枝长不再整齐重复。疏密间隙与向上回弯的枝梢，打破旧 Conifer 的规则层级。' },
  willow: { label: '垂柳', english: 'Weeping Willow', description: '从骨干树冠垂下的真实枝轴，形成长短不一的狭长叶帘；沿用现有风动，不生成苹果。' },
};
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const escape = text => String(text).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const ids = [...records.map(([name, seed]) => `${name}-${seed}`), 'apple-first-pass', 'mixed'];
const captured = new Map();
let priorManifest;
if (!args.length) {
  const prior = await readFile(output, 'utf8');
  priorManifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  for (const match of prior.matchAll(/data-capture="([a-z0-9-]+)" src="data:image\/png;base64,([A-Za-z0-9+/=]+)"/g)) {
    if (captured.has(match[1])) throw new Error(`Duplicate capture: ${match[1]}`);
    captured.set(match[1], Buffer.from(match[2], 'base64'));
  }
}
const manifest = [];
for (const id of ids) {
  const bytes = args.length ? await readFile(resolve(args[1], `${id}.png`)) : captured.get(id);
  if (!bytes || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    throw new Error(`Missing/invalid original PNG: ${id}`);
  }
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  if (width !== 1600 || height !== 1000) throw new Error(`Unexpected capture dimensions: ${id}`);
  const checksum = sha256(bytes);
  if (priorManifest && !priorManifest.some(item => item.id === id && item.sha256 === checksum)) {
    throw new Error(`Capture integrity check failed: ${id}`);
  }
  captured.set(id, bytes);
  manifest.push({ id, originalFilename: `${id}.png`, originalLocation: `target/tree-species-review/${id}.png`,
    role: id === 'apple-first-pass' ? 'superseded candidate reference' : 'native Release validation capture',
    width, height, format: 'PNG', bytes: bytes.length, sha256: checksum });
}
const gallery = records.map(([name, seed, segments, leaves, fruit]) => {
  const { label, english, description } = species[name];
  const id = `${name}-${seed}`;
  return `<figure class="tree-card" data-seed-card="${seed}"><button type="button" class="photo-button" data-zoom aria-label="放大${label} · 种子 ${seed} 全帧"><span class="crop"><img data-capture="${id}" src="{{CAPTURE:${id}}}" width="1600" height="1000" ${seed === 122 ? '' : 'loading="lazy" '}alt="${escape(label)}原生 Release 截图，种子 ${seed}，包含树冠与主干"></span></button><figcaption><h3>${label}</h3><span class="latin">${english} / SEED ${seed}</span><p>${description}</p><div class="metrics"><span>枝段 <strong>${segments}</strong></span><span>叶实例 <strong>${leaves.toLocaleString('en-US')}</strong></span><span>苹果位点 <strong>${fruit}</strong></span></div></figcaption></figure>`;
}).join('\n');
const counts = records.map(([name, seed, segments, leaves, fruit]) => `<tr><th scope="row">${species[name].label}</th><td>${seed}</td><td>${segments}</td><td>${leaves.toLocaleString('en-US')}</td><td>${fruit}</td></tr>`).join('\n');
const script = await readFile(resolve(directory, 'report.js'), 'utf8');
let html = await readFile(resolve(directory, 'report.template.html'), 'utf8');
html = html.replace('{{GALLERY}}', gallery).replace('{{COUNTS}}', counts)
  .replace('{{REPORT_SCRIPT}}', script)
  .replace('{{SCRIPT_HASH}}', createHash('sha256').update(script).digest('base64'));
html = html.replace(/\{\{CAPTURE:([a-z0-9-]+)\}\}/g, (_, id) => {
  if (!captured.has(id)) throw new Error(`Unknown capture: ${id}`);
  return `data:image/png;base64,${captured.get(id).toString('base64')}`;
});
if (/\{\{[A-Z_]+/.test(html)) throw new Error('Unresolved template placeholder');
if ([...html.matchAll(/data-capture="/g)].length !== ids.length) throw new Error('Capture count mismatch');
await writeFile(output, html);
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Built ${output}: ${Buffer.byteLength(html).toLocaleString('en-US')} bytes, ${ids.length} original PNGs embedded.`);
