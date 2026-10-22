import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {analyze,options,checkBudgets} from '../check-startup-cadence.mjs';

function fixture(fps=60,record=3300) {
  const rows=Array.from({length:17},(_,i)=>{
    const s=(86390+i)%86400;
    const clock=[Math.floor(s/3600),Math.floor(s/60)%60,s%60].map(x=>String(x).padStart(2,'0')).join(':');
    return `[${clock}.000 INFO test] [PERF][CPU_FRAME_SCOPE] frame ${i*fps} render.record=${record}us\n`;
  }).join('');
  return '[SCENE_PIXELS] screen=5120x2880 scene=1024x576\n'+rows+'[SHUTDOWN] phase=complete failures=0\n';
}

test('uses actual frame cadence across midnight, not GPU reciprocal',()=>{
  const result=analyze(fixture()+'[PERF][GPU_FRAME_SCOPE] frame 1 frame.render=1000us\n');
  assert.equal(result.fps,60);assert.equal(result.seconds,11);assert.equal(result.samples,12);
  assert.ok(Math.abs(result.recordMeanMs-3.3)<1e-10);
  assert.throws(()=>checkBudgets(analyze(fixture(25,25000)),{minFps:50}),/cadence 25/);
  assert.throws(()=>checkBudgets(result,{maxRecordMs:1}),/CPU recording/);
});
test('rejects incomplete, corrupt, resized and short evidence; retains warnings',()=>{
  assert.throws(()=>analyze(fixture().replace('failures=0','failures=1')),/shutdown/);
  assert.throws(()=>analyze(fixture()+'VUID-example'),/error/);
  assert.throws(()=>analyze(fixture()+'[SCENE_PIXELS] screen=800x600 scene=160x120'),/dimensions/);
  assert.throws(()=>analyze(fixture().replace('frame 60 ','frame 0 ')),/increase/);
  assert.throws(()=>analyze('[SCENE_PIXELS] screen=5x5 scene=1x1\n[SHUTDOWN] phase=complete failures=0'),/samples/);
  assert.equal(analyze(fixture()+'[WARN audio] retained warning\n').warnings,1);
});
test('budgets are explicit and argument errors are actionable',()=>{
  assert.deepEqual(options(['a.log']),{file:'a.log'});
  assert.deepEqual(options(['a.log','--min-fps','50','--max-record-ms','8']),{file:'a.log',minFps:50,maxRecordMs:8});
  for(const a of [[],['--unknown'],['a.log','--min-fps'],['a.log','--min-fps','NaN'],['a.log','--max-record-ms','0'],['a.log','--wat','1'],['a.log','--min-fps','5','--min-fps','8']]) assert.throws(()=>options(a));
});
test('real CLI returns red/green and usage exit codes without losing JSON evidence',()=>{
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'re-flora-cadence-'));
  const cli=new URL('../check-startup-cadence.mjs',import.meta.url).pathname;
  const run=args=>spawnSync(process.execPath,[cli,...args],{encoding:'utf8'});
  try {
    const file=path.join(dir,'run.log');fs.writeFileSync(file,fixture(25,25000));
    const red=run([file,'--min-fps','50']);assert.equal(red.status,1);assert.equal(JSON.parse(red.stdout).fps,25);assert.match(red.stderr,/cadence/);
    fs.writeFileSync(file,fixture());const green=run([file,'--min-fps','50','--max-record-ms','8']);assert.equal(green.status,0);assert.equal(JSON.parse(green.stdout).fps,60);
    const usage=run([file,'--max-record-ms','no']);assert.equal(usage.status,2);assert.match(usage.stderr,/Usage:/);
    const help=run(['--help']);assert.equal(help.status,0);assert.match(help.stdout,/--auto-exit 16/);
  } finally {fs.rmSync(dir,{recursive:true,force:true});}
});
