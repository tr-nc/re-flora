import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {comparisonConfig,validateCounts,summarize} from './validate-local-lighting.mjs';

test('day/night comparisons set one global policy and restore unrelated preferences',()=>{
  const source=readFileSync('config/gui.toml','utf8');
  for(const time of [0.45,0.95])for(const enabled of [false,true]) {
    const saved=comparisonConfig(source,time,enabled);
    assert.match(saved,new RegExp(`id = "local_lighting_enabled"[\\s\\S]*?value = ${enabled}`));
    assert.match(saved,new RegExp(`id = "time_of_day"[\\s\\S]*?value = ${time}`));
    assert.equal(comparisonConfig(saved,time,enabled),saved);
    assert.match(saved,/id = "grass_light_ris"[\s\S]*?value = false/);
    assert.match(saved,/id = "local_light_budget"[\s\S]*?value = 8/);
    assert.equal(source.includes('id = "local_lighting_enabled"'),true);
  }
});
test('zero light evidence is required in every disabled publication',()=>{
  const log=n=>`[LOCAL_LIGHT][LIVE] count=${n} capacity=64\n`;
  const exit='Application exited successfully';
  validateCounts(log(0)+exit,false);validateCounts(log(8)+exit,true);
  validateCounts(log(0)+exit,true,0); // GUI workbench has no emitters.
  assert.throws(()=>validateCounts(log(8)+log(0)+exit,false));
  assert.throws(()=>validateCounts(exit,false));
  assert.throws(()=>validateCounts(log(0)+exit,true));
  assert.throws(()=>validateCounts(log(8)+'[NIGHT_FLOWERS] clusters=1\n'+exit,true));
});
test('aggregate day/night labels cannot alias measured scene dimensions',()=>{
  const rows=[];
  for(const period of ['day','night'])for(const enabled of [true,false])for(const actualFps of [20,30,40])
    rows.push({period,enabled,scene:'320x180',screen:'1600x900',actualFps,grassGpuMs:enabled?12:5,wholeProfiledFps:actualFps});
  const summary=summarize(rows);
  assert.equal(summary.day.on.actualFps,30);assert.equal(summary.night.off.grassGpuMs,5);
  assert.throws(()=>summarize(rows.map((r,i)=>i===0?{...r,screen:'1920x1080'}:r)));
});
test('help is side-effect-free and errors carry a usable next command',()=>{
  const run=args=>spawnSync(process.execPath,['scripts/validate-local-lighting.mjs',...args],{encoding:'utf8'});
  const help=run(['--help']);assert.equal(help.status,0);assert.match(help.stdout,/--case perf\|capture/);
  for(const args of [[],['--case','invalid','--output','target/unused'],['--unknown']]) {
    const result=run(args);assert.equal(result.status,2);assert.match(result.stderr,/--output NEW_DIR/);
  }
});
