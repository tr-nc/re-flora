#!/usr/bin/env node
// Explicit Release save/load acceptance; does not belong in cargo test.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {crc32} from 'node:zlib';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';

const help = `Usage: node scripts/validate-flower-snapshots.mjs

Uses hidden, muted Release runs to save every authored flower species and verify
startup plus two runtime reloads. Also constructs explicit four-species and
four-plus-retired-Kochia copies to verify legacy migration through the real loader.
Requires Cargo, Slang, Vulkan desktop access and about 400 MiB of temporary disk.
Only writes target/flower-native-review/snapshots/. Never uses player save files
or saves GUI settings. Temporary terrain files are deleted on success; failures
retain them for inspection. Logs and summary.json remain. Exit: 0 pass/help, 1
runtime/validation failure, 2 usage error.`;
if (process.argv.length > 2) {
  if (process.argv.length === 3 && ['--help','-h'].includes(process.argv[2])) {console.log(help);process.exit(0);}
  console.error(`Unexpected arguments.\n${help}`);process.exit(2);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const output = path.join(root,'target/flower-native-review/snapshots');
fs.mkdirSync(output,{recursive:true});
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const gui = () => hash(fs.readFileSync(path.join(root,'config/gui.toml')));
const before = gui();
const summary = {configSha256:before,runs:[]};
const files = ['current','legacy-four','legacy-kochia'].map(name=>path.join(output,`${name}.rflterrain`));
function run(name,mode,flag,file) {
  const result = spawnSync('cargo',['run','--release','--','--hidden','--mute',flag,file,'--auto-exit','0.5'],
    {cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_GARDEN_SNAPSHOT_SMOKE:mode}});
  const log = `${result.stdout||''}\n${result.stderr||''}`;
  fs.writeFileSync(path.join(output,`${name}.log`),log);
  assert.equal(result.status,0,`${name}: ${result.error || result.signal || 'see log'}`);
  const diagnostics = log.split('\n').filter(s=>/\bERROR\b|VUID-|panicked at|Validation Error/.test(s));
  const platformWarnings = diagnostics.filter(s=>s.includes('sctk_adwaita::config') && s.includes('XDG Settings Portal'));
  assert.deepEqual(diagnostics.filter(s=>!platformWarnings.includes(s)),[],name);
  assert.ok(log.includes('[SHUTDOWN] phase=complete failures=0'),name);
  if (mode === 'verify') {
    assert.ok(log.includes('passed startup_and_repeated_runtime_load=true'),name);
    assert.equal((log.match(/terrain=exact flora=exact trees=exact no_duplicates=true/g)||[]).length,2,name);
  }
  summary.runs.push({name,platformWarnings,log:path.relative(root,path.join(output,`${name}.log`))});
  assert.equal(gui(),before,'GUI config changed');
  console.log(`PASS: ${name}`);
  return log;
}
try {
  run('save','seed','--terrain-save',files[0]);
  const fd = fs.openSync(files[0],'r');
  const header = Buffer.alloc(64);fs.readSync(fd,header,0,64,0);
  const chunkBytes = [28,32,36,40].reduce((n,i)=>n*header.readUInt32LE(i),1);
  const offset = header.readUInt32LE(12)+header.readUInt32LE(48)*(header.readUInt32LE(52)+chunkBytes);
  const footer = Buffer.alloc(12);fs.readSync(fd,footer,0,12,offset);
  const bytes = Buffer.alloc(Number(footer.readBigUInt64LE()));fs.readSync(fd,bytes,0,bytes.length,offset+12);fs.closeSync(fd);
  assert.equal(crc32(bytes),footer.readUInt32LE(8),'original garden checksum');
  const garden = JSON.parse(bytes);
  const counts = Object.fromEntries(garden.flora.chunks[0].species.map(s=>[s.key,0]));
  for (const chunk of garden.flora.chunks) for (const s of chunk.species) counts[s.key]+=s.instances.length;
  assert.equal(Object.keys(counts).length,12);
  assert.ok(Object.values(counts).slice(2).every(n=>n>0),'all ten authored species, including all eight model flowers, must be saved');
  summary.savedSpeciesCounts = counts;
  for (const [index,fixture] of files.slice(1).entries()) {
    const legacy = structuredClone(garden);
    for (const [chunkIndex,chunk] of legacy.flora.chunks.entries()) {
      chunk.species = chunk.species.slice(0,4);
      if (index === 1) chunk.species.push({key:'kochia',instances:[[0xff010203,42]],authored:[[10_000+chunkIndex,55]]});
    }
    const data = Buffer.from(JSON.stringify(legacy)), prefix = Buffer.alloc(12);
    prefix.writeBigUInt64LE(BigInt(data.length));prefix.writeUInt32LE(crc32(data),8);
    fs.copyFileSync(files[0],fixture,fs.constants.COPYFILE_FICLONE);
    const fd = fs.openSync(fixture,'r+');
    fs.writeSync(fd,prefix,0,prefix.length,offset);fs.writeSync(fd,data,0,data.length,offset+12);
    fs.ftruncateSync(fd,offset+12+data.length);fs.closeSync(fd);
  }
  for (const file of files) {
    const name = path.basename(file,'.rflterrain');
    const log = run(name,'verify','--terrain-load',file);
    if (name === 'legacy-kochia') assert.match(log,/removed retired Kochia plants=8/);
  }
  summary.result = 'passed';
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  for (const file of files) fs.rmSync(file);
  console.log(`PASS: current and both historical schemas; retained terrain/plants/trees exact.\n${output}/summary.json`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/. Retry: node scripts/validate-flower-snapshots.mjs`);process.exitCode=1;
} finally {
  if (gui() !== before) {console.error('GUI config changed; inspect config/gui.toml.');process.exitCode=1;}
}
