#!/usr/bin/env node
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const help=`Usage: node scripts/validate-stem-blocks.mjs [--seconds <positive-number>]
Hidden/muted Release captures: continuous surface A, block B, fine .25, coarse 4.
Then 8 live phases: A, fine, normal, coarse, wind, orbit, near clipping, return A;
also resize, six species draws, no head rebuilds, no GUI writes or Vulkan errors.
Default sweep 90s + four 4s captures. Requires Cargo/Slang/Vulkan and a display.
Artifacts: target/stem-blocks-review/{*.png,*.log,summary.json}.
Exit: 0 pass/help, 1 failed validation, 2 invalid usage.
Example: env -u WAYLAND_DISPLAY node scripts/validate-stem-blocks.mjs --seconds 90`;
const args=process.argv.slice(2);
if(args.length===1&&['-h','--help'].includes(args[0])){console.log(help);process.exit(0);}
const seconds=args.length===0?90:Number(args[1]);
if(!(args.length===0||(args.length===2&&args[0]==='--seconds'))||!Number.isFinite(seconds)||seconds<=0){console.error(help);process.exit(2);}
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),output=path.join(root,'target/stem-blocks-review');fs.mkdirSync(output,{recursive:true});
const sha=data=>createHash('sha256').update(data).digest('hex');
const config=()=>sha(fs.readFileSync(path.join(root,'config/gui.toml'))),before=config();
const speciesCount=JSON.parse(fs.readFileSync(path.join(root,'assets/models/flowers.json'))).flowers.length;
const summary={configSha256:before,speciesCount,runs:[]};
try{
  for(const mode of ['stem-blocks-a','stem-blocks-b','stem-blocks-fine','stem-blocks-coarse','stem-blocks']){
    const sweep=mode==='stem-blocks',label=sweep?'sweep':mode.slice('stem-blocks-'.length),image=path.join(output,label+'.png');
    const cli=['run','--release','--','--hidden','--mute','--windowed'];
    if(sweep)cli.push('--resize-lifecycle-test','--auto-exit',String(seconds));
    else{fs.rmSync(image,{force:true});cli.push('--screenshot','player-default',image,'--screenshot-delay','3','--auto-exit','4');}
    console.log(`Checking ${mode}…`);
    const result=spawnSync('cargo',cli,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_FLOWER_MODEL_REVIEW:mode}});
    const log=`${result.stdout||''}\n${result.stderr||''}`,logPath=path.join(output,label+'.log');fs.writeFileSync(logPath,log);
    assert.equal(result.status,0,`${mode}: ${result.error||result.signal||logPath}`);
    assert.deepEqual(log.split('\n').filter(l=>/\bERROR\b|panicked at|VUID-|Validation Error|Validation Warning/.test(l)),[],logPath);
    assert.match(log,/\[SHUTDOWN\] phase=complete failures=0/);
    assert.equal((log.match(/\[FLOWER_REVIEW_PLANT\]/g)||[]).length,speciesCount);
    assert.equal((log.match(/\[MODEL_CACHE_BUILD\] kind=3\b/g)||[]).length,1,'geometry changes rebuilt head bank');
    const run={mode,log:path.relative(root,logPath)};
    const phases=[...log.matchAll(/\[STEM_REVIEW_PHASE\] phase=(\d+) enabled=true method=2 .*geometry=(\w+) geometry_cells=([\d.]+) saved=false/g)];
    if(sweep){
      assert.deepEqual(phases.map(m=>+m[1]),Array.from({length:8},(_,i)=>i),`Incomplete sweep; retry --seconds ${Math.max(120,seconds*2)}`);
      assert.deepEqual(phases.map(m=>[m[2]==='true',+m[3]]),[[false,1],[true,.25],[true,1],[true,4],[true,1],[true,.25],[true,4],[false,1]]);
      run.phases=phases.map((m,i)=>{
        const segment=log.slice(m.index,phases[i+1]?.index),species=[...new Set([...segment.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m=>m[1]))];
        assert.equal(species.length,speciesCount,`phase ${i} actual draws`);
        return {phase:+m[1],geometry:m[2]==='true',cellScale:+m[3],species};
      });
      assert.match(log,/\[STEM_REVIEW_PHASE\] phase=4 .*freeze=false/);
      assert.match(log,/\[STEM_REVIEW_PHASE\] phase=5 .*motion=orbit/);
      assert.match(log,/\[STEM_REVIEW_PHASE\] phase=6 .*motion=near/);
      assert.match(log,/\[RESIZE_LIFECYCLE\] phase=published/);
    }else{
      const expected=label==='fine'?.25:label==='coarse'?4:1;
      assert.equal(phases.length,1);assert.equal(phases[0][2],String(label!=='a'));assert.equal(+phases[0][3],expected);
      const png=fs.readFileSync(image);assert.equal(png.subarray(1,4).toString(),'PNG');
      run.image=path.relative(root,image);run.dimensions=[png.readUInt32BE(16),png.readUInt32BE(20)];run.sha256=sha(png);
    }
    assert.equal(config(),before,'GUI settings changed');summary.runs.push(run);
  }
  assert.equal(new Set(summary.runs.slice(0,4).map(r=>r.sha256)).size,4,'captures identical');
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  console.log(`PASS: four captures, eight live geometry phases, six species, stable head bank and GUI, clean Vulkan logs.\n${output}/summary.json\nVisual approval and Release performance acceptance remain separate.`);
}catch(error){console.error(`${error.message}\nInspect ${output}`);process.exitCode=1;}
finally{if(config()!==before){console.error('GUI settings changed; inspect config/gui.toml');process.exitCode=1;}}
