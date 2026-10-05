#!/usr/bin/env node
// Release GPU runs are deliberately serial. CPU path work is separately timed.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';

const suites = ['all', 'gpu', 'cpu'];
const help = `Usage: node scripts/validate-stem-band-candidates.mjs [--suite ${suites.join('|')}] [--quick] [--pixels] [--resolution 8..512] [--scene CAMERA --grid ODD] [--phases]
Compare original voxel/block rendering and square color bands. GPU grass uses production painting (3x3 and
15x15); CPU fixtures animate branched flower-like and climbing-like paths.
Requires cargo build --release, Vulkan display and GPU timestamps. Runs serially,
hidden/muted, with fixed simulation time. Does not change saved settings.
--suite selects the source paths to test (default all).
--pixels adds the square model-pixelized candidate and focuses GPU coverage
on small-near and large-wide/far/low views. Use --suite gpu (or all), not cpu.
--resolution requests model-grid samples (8..512, default 45); legacy 8..31
requests normalize to the saved model-grid minimum 32. Artifact names, measurements
and fixture logs use the effective resolution; requested/effective values are both
recorded. Ordinary candidates force pixels off.
--scene selects one GPU camera: near-both, mid-both, wide-both, far-both, low-both,
  top-both or inside-both. Requires --suite gpu; --grid sets odd paint size 3..15
  (default 3 with --scene). --grid requires --scene.
--phases adds diagnostic prepare/atlas/display GPU timestamps to square-pixels
  only; requires --pixels. Extra timestamps and logs perturb timing: do not use
  diagnostic timings as primary acceptance. Runs fail if timestamp scopes drop;
  lower --grid or --resolution for very large diagnostic sweeps.
RE_FLORA_GRASS_BAND_POSE_REUSE=0 disables GPU pose reuse for a diagnostic comparison
(default 1). This choice is recorded in summary.json.
--quick uses one repeat and the high-population wide views; default uses two
repeats in opposite order, near/low/mid/far grass and CPU growth/near coverage.
Artifacts: target/stem-band-trials/{quick|full}-{suite}-square[-pixels]/{*.log,*.png,runs.json,summary.json}.
Custom scene/resolution/phase runs add their parameters to the artifact directory.
Reruns overwrite that suite's artifacts. No automatic visual or release acceptance.
Examples:
  node scripts/validate-stem-band-candidates.mjs --quick
  node scripts/validate-stem-band-candidates.mjs --suite gpu
  node scripts/validate-stem-band-candidates.mjs --suite gpu --pixels
  node scripts/validate-stem-band-candidates.mjs --suite gpu --pixels --scene near-both --grid 3 --resolution 128
  node scripts/validate-stem-band-candidates.mjs --suite gpu --pixels --scene wide-both --grid 15 --phases
Exit: 0 successful/help; 1 runtime/measurement failure; 2 invalid arguments.`;
let options;
try {
  options = parseArgs({options: {suite: {type:'string', default:'all'}, quick:{type:'boolean',default:false}, pixels:{type:'boolean',default:false}, resolution:{type:'string',default:'45'}, scene:{type:'string'}, grid:{type:'string'}, phases:{type:'boolean',default:false}, help:{type:'boolean',short:'h'}}}).values;
  if (options.help) { console.log(help); process.exit(0); }
  if (!suites.includes(options.suite)) throw new Error(`--suite must be ${suites.join(', ')}, got ${options.suite}`);
  if (options.pixels && options.suite==='cpu') throw new Error('--pixels tests GPU grass; retry --suite gpu --pixels');
  if (!/^\d+$/.test(options.resolution) || +options.resolution<8 || +options.resolution>512) throw new Error('--resolution must be an integer 8..512');
  options.requestedResolution=+options.resolution;
  options.resolution=Math.max(32,options.requestedResolution);
  if (options.scene && (options.suite!=='gpu' || !['near-both','mid-both','wide-both','far-both','low-both','top-both','inside-both'].includes(options.scene))) throw new Error('--scene requires --suite gpu and a camera from the list below');
  if (options.grid && !options.scene) throw new Error('--grid requires --scene; e.g. --suite gpu --scene near-both --grid 3');
  if (options.grid && (!/^\d+$/.test(options.grid) || +options.grid<3 || +options.grid>15 || +options.grid%2!==1)) throw new Error('--grid must be an odd integer 3..15');
  if (options.phases && !options.pixels) throw new Error('--phases requires --pixels; e.g. --suite gpu --pixels --scene wide-both --grid 15 --phases');
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }
const root = path.resolve(import.meta.dirname,'..');
const binary = path.join(root,'target/release/re-flora');
const suffix=`${options.scene?`-g${options.grid??3}-${options.scene}`:''}${options.resolution!==45?`-r${options.resolution}`:''}${options.phases?'-phases':''}`;
const output = path.join(root,`target/stem-band-trials/${options.quick?'quick':'full'}-${options.suite}-square${options.pixels?'-pixels':''}${suffix}`);
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
const baseModesGPU = [{name:'voxel',ab:'a'}, {name:'square',ab:'b'}];
const modesGPU = [...baseModesGPU, ...(options.pixels ? [
  {name:'square-pixels',ab:'b',pixels:true}] : [])];
const modesCPU = [{name:'blocks',ab:'a'}, {name:'square',ab:'b'}];
const jobs = [];
if(options.suite!=='cpu') {
  const scenes=options.scene ? [[+(options.grid??3),options.scene]] : options.quick ? [[15,'wide-both']] : options.pixels ? [
    [3,'near-both'],[15,'wide-both'],[15,'far-both'],[15,'low-both']] : [
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
const grassPoseReuse=process.env.RE_FLORA_GRASS_BAND_POSE_REUSE??'1';
try {
  assert.match(grassPoseReuse,/^[01]$/,'RE_FLORA_GRASS_BAND_POSE_REUSE must be 0 or 1');
  assert.ok(fs.existsSync(binary),'Release binary missing. Run cargo build --release, then retry this command.');
  fs.mkdirSync(output,{recursive:true});
  for(let repeat=0;repeat<(options.quick?1:2);repeat++) for(const job of jobs) {
    const modes=repeat%2===0?job.modes:job.modes.toReversed();
    for(const mode of modes) {
      const name=`${job.key}-${mode.name}-${repeat+1}`;
      console.log(`running ${name}`);
      const directory=path.join(output,name);fs.mkdirSync(directory,{recursive:true});
      const env={...process.env,RE_FLORA_GRASS_STEM_CAPTURE:directory,RE_FLORA_GRASS_BAND_POSE_REUSE:grassPoseReuse,
        RE_FLORA_GRASS_BAND_PIXELIZATION:mode.pixels?'1':'0',RE_FLORA_STEM_PIXEL_RESOLUTION:String(options.resolution)};
      for(const key of ['WAYLAND_DISPLAY','RE_FLORA_GRASS_STEM_REVIEW','RE_FLORA_CPU_STEM_REVIEW','RE_FLORA_CPU_STEM_NEAR','RE_FLORA_CLIMBING_REVIEW','RE_FLORA_GRASS_STEM_TRYOUT','RE_FLORA_FLOWER_MODEL_REVIEW','RE_FLORA_STEM_PIXEL_LIFECYCLE','RE_FLORA_STEM_PIXEL_PROFILE'])delete env[key];
      const phaseProfile=options.phases&&mode.name==='square-pixels';
      if(phaseProfile)env.RE_FLORA_STEM_PIXEL_PROFILE='1';
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
      if(mode.pixels)assert.match(log,/\[GRASS_MODEL_PIXELS\] enabled=true/);
      if(job.source==='gpu')assert.match(log,new RegExp(`FLOWER_STEM_SAMPLING\\].*model_resolution=${options.resolution}(?: |$)`),'Requested model-grid resolution did not reach normalized rendering');
      const marker=job.source==='gpu'?'GRASS_STEM_REVIEW':'CPU_STEM_REVIEW';
      const start=log.match(new RegExp(`\\[${marker}\\].*phase=sample app_frame=(\\d+)(.*)`));
      const end=log.match(new RegExp(`\\[${marker}\\].*phase=complete app_frame=(\\d+)(.*)`));
      assert.ok(start&&end,'Missing sample boundaries');
      const population=job.source==='gpu'?start[2].match(/grass=\[([^\]]+)\]/)[1]:start[2].match(/plants=(\d+) bands=(\d+)/).slice(1).join(', ');
      const scope=job.source==='gpu'?'graphics.flora':'graphics.cpu_stems';
      const metrics={[scope]:[],'frame.render':[]};
      if(job.source==='gpu')metrics['graphics.flora_lighting_cache']=[];
      if(phaseProfile)for(const phase of ['prepare','atlas','display'])metrics[`graphics.grass_pixels.${phase}`]=[];
      for(const line of log.split('\n')) {
        const frame=line.match(/GPU_FRAME_SCOPE\] frame (\d+).*dropped=(\d+) (.*)/);
        if(!frame||+frame[1]<+start[1]+4||+frame[1]>=+end[1])continue;
        assert.equal(+frame[2],0,'Dropped GPU scopes');
        for(const metric of Object.keys(metrics)) {
          const values=[...frame[3].matchAll(new RegExp(`(?:^| )${metric.replaceAll('.','\\.')}=(\\d+)us`,'g'))];
          assert.ok(values.length,`${name}: missing ${metric}`);
          // A phase repeats once per streamed batch: sum within the same frame
          // before taking percentiles, never treat batches as frame samples.
          metrics[metric].push(values.reduce((sum,value)=>sum+ +value[1],0));
        }
      }
      assert.ok(metrics[scope].length>=(options.quick&&job.source==='gpu'?100:250),'Insufficient GPU samples');
      if(job.source==='gpu')metrics['grass.render_total']=metrics['graphics.flora'].map((value,index)=>value+metrics['graphics.flora_lighting_cache'][index]);
      const cpu={path:[],upload:[],record:[],submit:[]};
      for(const line of log.split('\n')) {
        const frame=line.match(/CPU_FRAME_SCOPE\] frame (\d+) (.*)/);
        if(!frame||+frame[1]<+start[1]+4||+frame[1]>=+end[1])continue;
        for(const [kind,scope] of [['record','render.trace_record'],['submit','render.submit_present']]) {
          const value=frame[2].match(new RegExp(`(?:^| )${scope.replaceAll('.','\\.')}=(\\d+)us`));
          assert.ok(value,`${name}: missing CPU ${scope}`);cpu[kind].push(+value[1]);
        }
      }
      if(job.source==='cpu')for(const match of log.matchAll(/CPU_STEM_TIMING\] simulation_frame=(\d+) path_us=(\d+) upload_us=(\d+)/g)) {
        if(+match[1]>=124&&+match[1]<420){cpu.path.push(+match[2]);cpu.upload.push(+match[3]);}
      }
      const image=fs.readFileSync(path.join(directory,`${caseName}.png`));
      assert.equal(image.subarray(1,4).toString(),'PNG');
      const dimensions=[image.readUInt32BE(16),image.readUInt32BE(20)];
      assert.deepEqual(dimensions,[2560,1440],'Resolution changed; do not compare mixed resolutions');
      const row={key:job.key,source:job.source,mode:mode.name,repeat:repeat+1,grassPixelization:!!mode.pixels,phaseProfile,requestedStemResolution:options.requestedResolution,stemResolution:options.resolution,population,dimensions,
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
      for(const kind of ['record','submit',...(job.source==='cpu'?['path','upload']:[])])metrics[`cpu.${kind}`]=summarize(selected.flatMap(row=>row.cpu[kind]));
      return [mode.name,metrics];
    }))};
  });
  const revision=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).stdout.trim();
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify({revision,grassPoseReuse:grassPoseReuse==='1',pixelCandidates:options.pixels,phaseProfile:options.phases,requestedStemResolution:options.requestedResolution,stemResolution:options.resolution,quick:options.quick,repeats:options.quick?1:2,savedConfigSha256:before,summary},null,2));
  console.log(JSON.stringify(summary,null,2));
} catch(error) {
  console.error(`${error.message}\nArtifacts: ${output}\nInspect the failed log; cargo run --release -- --tail-latest-log 200`);
  process.exitCode=1;
} finally { checkSaved(); }
