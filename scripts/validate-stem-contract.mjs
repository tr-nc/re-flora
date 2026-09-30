#!/usr/bin/env node
// Real Release source-contract A/B, using the same declarative saved GUI fields.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const help=`Usage: node scripts/validate-stem-contract.mjs [--seconds <positive-number>]

Captures angular A / fixed-object B at near and 4x farther camera distances,
then exercises 12 live phases: paired 1x/2x/4x dolly, B fixed-position turn,
orbit, wind, 32/512 source pixels, resize and return to A. Default sweep: 70s.
Captures take 4s each. Requires Cargo/Slang/Vulkan and a desktop display.
Runs hidden/muted Release, never saves GUI settings or publishes assets.
Artifacts: target/stem-contract-review/{*.png,*.log,summary.json}.
Exit codes: 0 passed/help; 1 validation/runtime failure; 2 invalid usage.
Example: env -u WAYLAND_DISPLAY node scripts/validate-stem-contract.mjs --seconds 70`;
const args=process.argv.slice(2);
if(args.length===1&&['-h','--help'].includes(args[0])){console.log(help);process.exit(0);}
const seconds=args.length===0?70:Number(args[1]);
if(!(args.length===0||(args.length===2&&args[0]==='--seconds'))||!Number.isFinite(seconds)||seconds<=0){console.error(`Expected --seconds <positive-number>.\n${help}`);process.exit(2);}
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),output=path.join(root,'target/stem-contract-review');
fs.mkdirSync(output,{recursive:true});
const sha=data=>createHash('sha256').update(data).digest('hex');
const config=()=>sha(fs.readFileSync(path.join(root,'config/gui.toml'))),before=config();
const speciesCount=JSON.parse(fs.readFileSync(path.join(root,'assets/models/flowers.json'))).flowers.length;
const summary={configSha256:before,speciesCount,runs:[]};
try{
  for(const mode of ['stem-contract-a-near','stem-contract-b-near','stem-contract-a-far','stem-contract-b-far','stem-contract']){
    const sweep=mode==='stem-contract',label=sweep?'sweep':mode.slice('stem-contract-'.length),image=path.join(output,label+'.png');
    const cli=['run','--release','--','--hidden','--mute','--windowed'];
    if(sweep)cli.push('--resize-lifecycle-test','--auto-exit',String(seconds));
    else{fs.rmSync(image,{force:true});cli.push('--screenshot','player-default',image,'--screenshot-delay','3','--auto-exit','4');}
    console.log(`Checking ${mode}…`);
    const result=spawnSync('cargo',cli,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_FLOWER_MODEL_REVIEW:mode}});
    const log=`${result.stdout||''}\n${result.stderr||''}`,logPath=path.join(output,label+'.log');fs.writeFileSync(logPath,log);
    assert.equal(result.status,0,`${mode}: ${result.error||result.signal||logPath}`);
    assert.deepEqual(log.split('\n').filter(line=>/\bERROR\b|panicked at|VUID-|Validation Error|Validation Warning/.test(line)),[],`render errors: ${logPath}`);
    assert.match(log,/\[SHUTDOWN\] phase=complete failures=0/);
    assert.equal((log.match(/\[FLOWER_REVIEW_PLANT\]/g)||[]).length,speciesCount);
    assert.equal((log.match(/\[MODEL_CACHE_BUILD\] kind=3\b/g)||[]).length,1,'A/B or source resolution rebuilt the head bank');
    const run={mode,log:path.relative(root,logPath)};
    if(sweep){
      const phases=[...log.matchAll(/\[STEM_CONTRACT_PHASE\] phase=(\d+) object=(\w+) pixels=(\d+) direction=(\d+) views=(\d+) distance=([\d.e+-]+)/g)];
      assert.deepEqual(phases.map(m=>+m[1]),Array.from({length:12},(_,i)=>i),`Incomplete sweep; retry --seconds ${Math.max(100,seconds*2)}`);
      run.phases=phases.map((m,i)=>{
        const segment=log.slice(m.index,phases[i+1]?.index);
        const species=[...new Set([...segment.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m=>m[1]))];assert.equal(species.length,speciesCount,`phase ${i} actual draws`);
        return {phase:+m[1],object:m[2]==='true',pixels:+m[3],direction:+m[4],views:+m[5],distance:+m[6],species};
      });
      for(const i of [0,2,4]){
        assert.equal(run.phases[i].object,false);assert.equal(run.phases[i+1].object,true);
        assert.equal(run.phases[i].distance,run.phases[i+1].distance);
        assert.equal(run.phases[i+1].pixels,256);assert.equal(run.phases[i+1].views,256);
      }
      assert.ok(Math.abs(run.phases[2].distance/run.phases[0].distance-2)<1e-5);
      assert.ok(Math.abs(run.phases[4].distance/run.phases[0].distance-4)<1e-5);
      assert.equal(run.phases[9].pixels,32);assert.equal(run.phases[10].pixels,512);assert.equal(run.phases[11].object,false);
      for(const phase of [6,7]){
        const eyes=[...log.matchAll(new RegExp(`\\[STEM_REVIEW_CAMERA\\] phase=${phase} .*eye=(\\[[^\\]]+\\]) focus=(\\[[^\\]]+\\])`,'g'))];
        assert.ok(eyes.length>=3);assert.equal(new Set(eyes.map(m=>m[1])).size===1,phase===6);
      }
      assert.match(log,/\[RESIZE_LIFECYCLE\] phase=published/);
    }else{
      assert.match(log,new RegExp(`\\[STEM_REVIEW_PHASE\\].*object=${mode.includes('-b-')} object_pixels=256`));
      const png=fs.readFileSync(image);assert.equal(png.subarray(1,4).toString(),'PNG');
      run.image=path.relative(root,image);run.dimensions=[png.readUInt32BE(16),png.readUInt32BE(20)];assert.ok(run.dimensions.every(n=>n>0));run.sha256=sha(png);
    }
    assert.equal(config(),before,'diagnostic saved GUI settings');summary.runs.push(run);
  }
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  console.log(`PASS: 4 near/far A/B captures, 12 live phases, all ${speciesCount} flowers, original source budget at 1x/2x/4x, no head rebuilds or Vulkan errors.\n${output}/summary.json\nSource-mask invariance is separately tested by flower_stem_object_grid.slang; screen subpixel aliasing is not eliminated.`);
}catch(error){console.error(`${error.message}\nInspect ${output}`);process.exitCode=1;}
finally{if(config()!==before){console.error('GUI settings changed; inspect config/gui.toml');process.exitCode=1;}}
