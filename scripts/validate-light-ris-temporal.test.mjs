import {test} from 'node:test';
import assert from 'node:assert/strict';
import {temporalMetrics,assertStable} from './validate-light-ris-temporal.mjs';

const row=(pool,jump)=>`[LIGHT_RIS][TEMPORAL] records=100 matched=100 stationary=90 lit_stationary=20 changed=${jump?10:0} pool_changed=${pool} max_relative_delta=${jump} mean_error_delta=0.01`;
const log=(pool,jump)=>Array.from({length:20},()=>row(pool,jump)).join('\n')+'\nApplication exited successfully';
test('static real-output flicker is rejected independently of startup and mean energy',()=>{
  assertStable(temporalMetrics(log(0,0)));
  assert.throws(()=>assertStable(temporalMetrics(log(80,10))),/candidate pools/);
  assert.throws(()=>assertStable(temporalMetrics(log(0,0.5))),/lighting jumped/);
});
test('missing, unlit, failed and contaminated evidence cannot pass',()=>{
  assert.throws(()=>temporalMetrics('Application exited successfully'),/Missing repeated/);
  const m=temporalMetrics(log(0,0));
  assert.throws(()=>assertStable({...m,litStationary:0}),/coverage/);
  assert.throws(()=>temporalMetrics(log(0,0)+'\nERROR invalid GPU output'));
});
