import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {AIR_TOTAL_SCOPES,options} from '../benchmark-sprinkler-air.mjs';

test('air scope includes the new producer and BOTH versions of the volumetric path',()=>{
  assert.deepEqual(AIR_TOTAL_SCOPES,['sprinkler_mist.pass','sprinkler_air.field','god_ray.air','god_ray.pass','god_ray_temporal.pass']);
  assert.equal(options([]).counts,'1,16');
  assert.equal(options([]).output,'target/sprinkler-air-review/measurement');
  assert.equal(options(['--counts','16','--output','target/test']).output,'target/test');
  for(const args of [['--counts','0'],['--counts','16,16'],['--repeats','1'],['--seconds','1'],['--duty','70']])
    assert.throws(()=>options(args));
});
test('air CLI help and usage failures are self-contained without launching',()=>{
  const script=new URL('../benchmark-sprinkler-air.mjs',import.meta.url).pathname;
  const help=spawnSync(process.execPath,[script,'--help'],{encoding:'utf8'});
  assert.equal(help.status,0);assert.match(help.stdout,/APPROXIMATE/);assert.match(help.stdout,/NEW\/empty/);
  const bad=spawnSync(process.execPath,[script,'--counts','0'],{encoding:'utf8'});
  assert.equal(bad.status,2);assert.match(bad.stderr,/--counts expects/);assert.match(bad.stderr,/Usage:/);
});
