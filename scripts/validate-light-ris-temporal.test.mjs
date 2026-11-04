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
test('motion statistics retain lit coverage and camera checks cover the whole sequence',()=>{
  const moving=log(0,0).replaceAll('mean_error_delta=0.01','mean_error_delta=0.01 lit_matched=20 moving_lit=7 max_moving_error_delta=0.003 mean_lit_error_delta=0.002 max_moving_total_error_delta=0.0004');
  const m=temporalMetrics(moving);
  assert.equal(m.litMatched,200);
  assert.equal(m.movingLit,70);
  assert.equal(m.maxMovingErrorDelta,0.003);
  assert.equal(m.maxMovingTotalErrorDelta,0.0004);
  assert.throws(()=>assertStable({...m,maxMovingTotalErrorDelta:0.3}),/total reference illumination/);
  assert.ok(Math.abs(m.meanLitErrorDelta-0.002)<1e-9);
  const transition=[...Array(10).fill(row(80,10)),...Array(10).fill(row(0,0))].join('\n')+'\nApplication exited successfully';
  assertStable(temporalMetrics(transition));
  assert.throws(()=>assertStable(temporalMetrics(transition,true)),/candidate pools/);
});
test('missing, unlit, failed and contaminated evidence cannot pass',()=>{
  assert.throws(()=>temporalMetrics('Application exited successfully'),/Missing repeated/);
  const m=temporalMetrics(log(0,0));
  assert.throws(()=>assertStable({...m,litStationary:0}),/coverage/);
  assert.throws(()=>temporalMetrics(log(0,0)+'\nERROR invalid GPU output'));
});
