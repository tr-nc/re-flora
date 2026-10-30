import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
const root=process.cwd(), out=path.resolve(process.argv[2]);
fs.mkdirSync(out,{recursive:true});
for(const binary of process.argv.slice(3)) for(const scene of ['near','far','low','inside']) {
 const name=path.basename(binary)+'-'+scene, dir=path.join(out,name), runtime=path.join(out,'runtime');
 fs.rmSync(runtime,{recursive:true,force:true});fs.mkdirSync(dir,{recursive:true});fs.mkdirSync(runtime,{recursive:true});fs.cpSync('config',path.join(runtime,'config'),{recursive:true});fs.symlinkSync(path.join(root,'assets'),path.join(runtime,'assets'));
 const env={...process.env};for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];delete env.WAYLAND_DISPLAY;
 Object.assign(env,{RE_FLORA_ROOT:runtime,RE_FLORA_GRASS_STEM_CAPTURE:dir,RE_FLORA_GRASS_STEM_REVIEW:`${scene}-both-b`,RE_FLORA_GRASS_STEM_GRID:'3',RE_FLORA_STEM_SAMPLE_FRAMES:'120'});
 console.log(name);
 const run=spawnSync(path.resolve(binary),['--hidden','--mute','--windowed','--perf','--authored-flora-bench'],{env,encoding:'utf8',maxBuffer:128*1024*1024});const log=run.stdout+run.stderr;fs.writeFileSync(path.join(out,name+'.log'),log);assert.equal(run.status,0);assert.doesNotMatch(log,/\bERROR\b|VUID-|SYNC-HAZARD|panicked at/);assert.match(log,/phase=complete failures=0/);assert(fs.existsSync(path.join(dir,`${scene}-both-b.png`)));
}fs.rmSync(path.join(out,'runtime'),{recursive:true,force:true});
