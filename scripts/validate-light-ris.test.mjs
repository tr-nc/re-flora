import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {setSavedValue,auditMetrics,perfMetrics,scalingMetrics,nativeArguments} from './validate-light-ris.mjs';
const cli=(...args)=>spawnSync(process.execPath,['scripts/validate-light-ris.mjs',...args],{encoding:'utf8'});

test('help and invalid invocations explain recovery without starting a game',()=>{
  assert.equal(cli('--help').status,0);
  for(const args of [['--case','oops'],['--seconds','0'],['--case','audit','--repeats','2'],['--unknown']]) {
    const run=cli(...args);assert.equal(run.status,2);assert.match(run.stderr,/Usage:.*validate-light-ris/);
  }
});
test('native launch arguments respect scenario and screenshot exclusions',()=>{
  const scaling=nativeArguments('scaling',180,['--perf','--environment-lighting-test-scene','local-light-scaling']);
  assert.ok(!scaling.includes('--garden-seed') && !scaling.includes('--camera-snapshot'));
  const capture=nativeArguments('capture',10,['--screenshot','garden-arrival','target/example.png','--screenshot-delay','6']);
  assert.ok(capture.includes('--garden-seed') && !capture.includes('--camera-snapshot'));
  for(const kind of ['audit','perf'])assert.ok(nativeArguments(kind,20).includes('--camera-snapshot'));
});
test('saved changes are targeted and preserve all unrelated config bytes',()=>{
  const base=readFileSync('config/gui.toml','utf8');
  const changed=setSavedValue(base,'grass_light_ris','true');
  assert.notEqual(changed,base);
  assert.equal(setSavedValue(changed,'grass_light_ris','false'),base);
  assert.throws(()=>setSavedValue(base,'missing_setting','true'));
});
const auditLine=(risNonzero=12,mismatch=0)=>`[00:01:02.000 INFO audit] [LIGHT_RIS][RECEIVERS] samples=100 non_finite=0 rendered_mismatches=${mismatch} reference_nonzero=10 ris_nonzero=${risNonzero} reference_rgb=[1, 2, 3] ris_rgb=[1.1, 1.9, 3]\n`;
test('receiver evidence rejects zero-output alias regression and incorrect real output',()=>{
  const text=auditLine().repeat(3)+'Application exited successfully';
  assert.equal(auditMetrics(text).samples,300);
  assert.throws(()=>auditMetrics(auditLine(0).repeat(3)+'Application exited successfully'),/nonzero direct lighting/);
  assert.throws(()=>auditMetrics(auditLine(12,1).repeat(3)+'Application exited successfully'),/mismatching/);
});
test('perf uses actual frame/wall time and rejects audit or missing samples',()=>{
  let text='screen=1920x1080 scene=384x216\n[LOCAL_LIGHT][LIVE] count=8 capacity=64\n';
  for(let i=0;i<90;i++) {
    const stamp=`[00:00:${String(Math.floor(i/10)).padStart(2,'0')}.${String(i%10*100).padStart(3,'0')} INFO timing]`;
    text+=`${stamp} [PERF][FRAME] frame ${i} total 40ms\n`;
    text+=`${stamp} [GPU_FRAME_SCOPE] frame ${i} graphics.flora_lighting_cache=5000us\n`;
  }
  text+='Application exited successfully';
  const m=perfMetrics(text);assert.equal(m.grassGpuMs,5);assert.equal(m.actualFps,10);
  assert.throws(()=>perfMetrics(text+' [LIGHT_RIS][RECEIVERS]'),/audit/);
  assert.throws(()=>perfMetrics(text,500),/GPU samples/);
});
test('many-light evidence requires completed removal and actual accepted uploads',()=>{
  let text='Application exited successfully\n[LOCAL_LIGHT_SCALE] complete final_zero=true\n';
  for(const n of [1,2,4,8,16,32,64,0])text+=`[LOCAL_LIGHT_SCALE] count=${n} samples=64 accepted=${n} overflow=0 raster_flora_cache_gpu_p50_us=1000.0\n`;
  assert.equal(scalingMetrics(text).length,8);
  assert.throws(()=>scalingMetrics(text.replace('count=64 samples=64 accepted=64','count=64 samples=64 accepted=8')),/actually uploaded/);
  assert.throws(()=>scalingMetrics(text.replace('[LOCAL_LIGHT_SCALE] complete','incomplete')),/timed out/);
});
