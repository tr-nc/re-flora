#!/usr/bin/env node
// Release GPU runs are deliberately serial. CPU path work is separately timed.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';

const suites = ['all', 'gpu', 'cpu'];
const help = `Usage: node scripts/validate-stem-band-candidates.mjs [--suite ${suites.join('|')}] [--quick]
Compare original voxel/block rendering, analytic grass, square bands, tapered
square bands and crossed ribbons. GPU grass uses production painting (3x3 and
15x15); CPU fixtures animate branched flower-like and climbing-like paths.
Requires cargo build --release, Vulkan display and GPU timestamps. Runs serially,
hidden/muted, with fixed simulation time. Does not change saved settings.
--suite selects the source paths to test (default all).
--quick uses one repeat and the high-population wide views; default uses two
repeats in opposite order, near/low/mid/far grass and CPU growth/near coverage.
Artifacts: target/stem-band-trials/{quick|full}-{suite}/{*.log,*.png,runs.json,summary.json}.
Reruns overwrite that suite's artifacts. No automatic visual or release acceptance.
Examples:
  node scripts/validate-stem-band-candidates.mjs --quick
  node scripts/validate-stem-band-candidates.mjs --suite gpu
Exit: 0 successful/help; 1 runtime/measurement failure; 2 invalid arguments.`;
let options;
try {
  options = parseArgs({options: {suite: {type:'string', default:'all'}, quick:{type:'boolean',default:false}, help:{type:'boolean',short:'h'}}}).values;
  if (options.help) { console.log(help); process.exit(0); }
  if (!suites.includes(options.suite)) throw new Error(`--suite must be ${suites.join(', ')}, got ${options.suite}`);
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }
const root = path.resolve(import.meta.dirname,'..');
const binary = path.join(root,'target/release/re-flora');
const output = path.join(root,`target/stem-band-trials/${options.quick?'quick':'full'}-${options.suite}`);
const hash = data => createHash('sha256').update(data).digest('hex');
const saved = ['config/gui.toml','config/camera_snapshots.toml'];
const before = saved.map(file=>hash(fs.readFileSync(path.join(root,file))));
const checkSaved = () => assert.deepEqual(saved.map(file=>hash(fs.readFileSync(path.join(root,file)))),before,'Saved user config changed');
const percentile = (values,fraction) => {
  assert.ok(values.length>0,'No timing samples');
  const sorted=values.toSorted((a,b)=>a-b);
  return sorted[Math.min(sorted.length-1,Math.floor(sorted.length*fraction))];
};
const summarize = values => ({samples:values.length,p50_us:percentile(values,.5),p95_us:percentile(values,.95)});
const modesGPU = [{name:'voxel',mode:0,ab:'a'}, {name:'analytic',mode:0,ab:'b'},
  {name:'square',mode:1,ab:'b'}, {name:'taper',mode:2,ab:'b'}, {name:'ribbon',mode:3,ab:'b'}];
const modesCPU = [{name:'blocks',mode:0,ab:'a'}, ...modesGPU.slice(2)];
const jobs = [];
if(options.suite!=='cpu') {
  const scenes=options.quick ? [[15,'wide-both']] : [
    [3,'near-both'],[3,'mid-both'],[3,'far-both'],[3,'low-both'],
    [15,'near-both'],[15,'wide-both'],[15,'far-both'],[15,'low-both']];
  for(const [grid,scene] of scenes) jobs.push({source:'gpu',key:`gpu-g${grid}-${scene}`,grid,scene,modes:modesGPU});
}
if(options.suite!=='gpu') {
  const scenes=options.quick ? [['flower',1024,false],['vine',1024,false]] : [
    ['flower',256,false],['flower',1024,false],['vine',256,false],['vine',1024,false],
    ['vine',1024,true],['growth',64,false]];
  for(const [shape,count,near] of scenes) jobs.push({source:'cpu',key:`cpu-${shape}-${count}${near?'-near':''}`,shape,count,near,modes:modesCPU});
}
const runs=[];
try {
  assert.ok(fs.existsSync(binary),'Release binary missing. Run cargo build --release, then retry this command.');
  fs.mkdirSync(output,{recursive:true});
  for(let repeat=0;repeat<(options.quick?1:2);repeat++) for(const job of jobs) {
    const modes=repeat%2===0?job.modes:job.modes.toReversed();
    for(const mode of modes) {
      const name=`${job.key}-${mode.name}-${repeat+1}`;
      console.log(`running ${name}`);
      const directory=path.join(output,name);fs.mkdirSync(directory,{recursive:true});
      const env={...process.env,RE_FLORA_STEM_BAND_MODE:String(mode.mode),RE_FLORA_GRASS_STEM_CAPTURE:directory};
      for(const key of ['WAYLAND_DISPLAY','RE_FLORA_GRASS_STEM_REVIEW','RE_FLORA_CPU_STEM_REVIEW','RE_FLORA_CPU_STEM_NEAR','RE_FLORA_CLIMBING_REVIEW','RE_FLORA_GRASS_STEM_TRYOUT','RE_FLORA_FLOWER_MODEL_REVIEW'])delete env[key];
      let caseName;
      if(job.source==='gpu') {
        caseName=`${job.scene}-${mode.ab}`;
        env.RE_FLORA_GRASS_STEM_REVIEW=caseName;env.RE_FLORA_GRASS_STEM_GRID=String(job.grid);
        env.RE_FLORA_STEM_SAMPLE_FRAMES=options.quick?'120':'300';
      } else {
        caseName=`${job.shape}-${job.count}-${mode.ab}`;env.RE_FLORA_CPU_STEM_REVIEW=caseName;
        if(job.near)env.RE_FLORA_CPU_STEM_NEAR='1';
      }
      const result=spawnSync(binary,['--hidden','--mute','--windowed','--perf','--authored-flora-bench'],{
        cwd:root,env,encoding:'utf8',maxBuffer:128*1024*1024,timeout:180000});
      const log=`${result.stdout??''}${result.stderr??''}`;
      fs.writeFileSync(path.join(output,`${name}.log`),log);
      assert.equal(result.status,0,`${name}: ${result.error??log.slice(-3000)}; inspect its log and retry --suite ${job.source}`);
      assert.ok(!/\bERROR\b|VUID|panicked at|Validation Error|Validation Warning/.test(log),`${name}: rendering errors`);
      assert.match(log,/\[SHUTDOWN\] phase=complete failures=0/);
      const marker=job.source==='gpu'?'GRASS_STEM_REVIEW':'CPU_STEM_REVIEW';
      const start=log.match(new RegExp(`\\[${marker}\\].*phase=sample app_frame=(\\d+)(.*)`));
      const end=log.match(new RegExp(`\\[${marker}\\].*phase=complete app_frame=(\\d+)(.*)`));
      assert.ok(start&&end,'Missing sample boundaries');
      const population=job.source==='gpu'?start[2].match(/grass=\[([^\]]+)\]/)[1]:start[2].match(/plants=(\d+) bands=(\d+)/).slice(1).join(', ');
      const scope=job.source==='gpu'?'graphics.flora':'graphics.cpu_stems';
      const metrics={[scope]:[],'frame.render':[]};
      for(const line of log.split('\n')) {
        const frame=line.match(/GPU_FRAME_SCOPE\] frame (\d+).*dropped=(\d+) (.*)/);
        if(!frame||+frame[1]<+start[1]+4||+frame[1]>=+end[1])continue;
        assert.equal(+frame[2],0,'Dropped GPU scopes');
        for(const metric of Object.keys(metrics)) {
          const value=frame[3].match(new RegExp(`(?:^| )${metric.replaceAll('.','\\.')}=(\\d+)us`));
          assert.ok(value,`${name}: missing ${metric}`);metrics[metric].push(+value[1]);
        }
      }
      assert.ok(metrics[scope].length>=(options.quick&&job.source==='gpu'?100:250),'Insufficient GPU samples');
      const cpu={path:[],upload:[]};
      if(job.source==='cpu')for(const match of log.matchAll(/CPU_STEM_TIMING\] simulation_frame=(\d+) path_us=(\d+) upload_us=(\d+)/g)) {
        if(+match[1]>=124&&+match[1]<420){cpu.path.push(+match[2]);cpu.upload.push(+match[3]);}
      }
      const image=fs.readFileSync(path.join(directory,`${caseName}.png`));
      assert.equal(image.subarray(1,4).toString(),'PNG');
      const dimensions=[image.readUInt32BE(16),image.readUInt32BE(20)];
      assert.deepEqual(dimensions,[2560,1440],'Resolution changed; do not compare mixed resolutions');
      const row={key:job.key,source:job.source,mode:mode.name,repeat:repeat+1,population,dimensions,
        image:path.relative(root,path.join(directory,`${caseName}.png`)),imageSha256:hash(image),metrics,cpu};
      runs.push(row);checkSaved();
      fs.writeFileSync(path.join(output,'runs.json'),JSON.stringify(runs,null,2));
      console.log(`  ${population}; ${scope} p50=${percentile(metrics[scope],.5)}us`);
    }
  }
  const summary=jobs.map(job=>{
    const rows=runs.filter(row=>row.key===job.key);
    assert.equal(new Set(rows.map(row=>row.population)).size,1,`${job.key}: candidate populations differ`);
    return {key:job.key,source:job.source,population:rows[0].population,modes:Object.fromEntries(job.modes.map(mode=>{
      const selected=rows.filter(row=>row.mode===mode.name);
      const metrics=Object.fromEntries(Object.keys(selected[0].metrics).map(metric=>[metric,summarize(selected.flatMap(row=>row.metrics[metric]))]));
      if(job.source==='cpu')for(const kind of ['path','upload'])metrics[`cpu.${kind}`]=summarize(selected.flatMap(row=>row.cpu[kind]));
      return [mode.name,metrics];
    }))};
  });
  const revision=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).stdout.trim();
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify({revision,quick:options.quick,repeats:options.quick?1:2,savedConfigSha256:before,summary},null,2));
  console.log(JSON.stringify(summary,null,2));
} catch(error) {
  console.error(`${error.message}\nArtifacts: ${output}\nInspect the failed log; cargo run --release -- --tail-latest-log 200`);
  process.exitCode=1;
} finally { checkSaved(); }
