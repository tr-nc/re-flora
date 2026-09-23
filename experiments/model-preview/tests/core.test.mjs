import test from 'node:test';
import assert from 'node:assert/strict';
import {projectedCoverage,repairCoverage} from '../connectivity.mjs';
import {repairImage,quantizeImage} from '../postprocess.mjs';
import {sampleTime,advanceTime} from '../timeline.mjs';
const full={shares:()=>true};

test('diagonal contact is complete; repair only the missing diagonal',()=>{
  for(const original of [[1,0,0,1],[0,1,1,0]]) assert.equal(repairCoverage(original,full,2).added,0);
  const original=Array(25).fill(0);original[0]=original[24]=1;
  const result=repairCoverage(original,full,5);
  assert.deepEqual([...result.additions].flatMap((v,i)=>v?[i]:[]),[6,12,18]);
  assert.equal(result.after,1);
});

test('empty or already connected tiles are untouched',()=>{
  for(const original of [Array(9).fill(0),Array(9).fill(1),[0,0,0,0,1,0,0,0,0]]) {
    assert.equal(repairCoverage(original,full,3).added,0);
  }
});

test('coverage supports 156 triangles without wrapping triangle identities at 32',()=>{
  const left=[[.1,.1,0],[1.9,.1,0],[.1,4.9,0]],right=[[3.1,.1,0],[4.9,.1,0],[4.9,4.9,0]];
  const triangles=Array.from({length:156},(_,i)=>i===155?right:left);
  const coverage=projectedCoverage(triangles,5);
  assert.ok(coverage.has(0));assert.ok(coverage.has(4));assert.equal(coverage.shares(0,4),false);
  const original=Array(25).fill(0);original[0]=original[4]=1;
  assert.equal(repairCoverage(original,coverage,5).added,0);
});

test('different repair groups never bridge to one another; existing RGBA is retained',()=>{
  const rgba=new Uint8Array(3*3*4),owners=new Uint32Array(9);
  rgba.set([20,50,80,255],0);rgba.set([90,40,10,255],32);owners[0]=1;owners[8]=2;
  const triangles=[[[0,0,0],[3,0,0],[3,3,0]],[[0,0,0],[3,3,0],[0,3,0]]];
  const split=repairImage(rgba,owners,[{id:1,triangles},{id:2,triangles}],3);
  assert.equal(split.added,0);assert.deepEqual(split.rgba,rgba);
  owners[8]=1;
  const joined=repairImage(rgba,owners,[{id:1,triangles}],3);
  assert.equal(joined.added,1);assert.equal(joined.rgba[19],255);
  assert.deepEqual(joined.rgba.subarray(0,4),rgba.subarray(0,4));
  assert.deepEqual(joined.rgba.subarray(32,36),rgba.subarray(32,36));
  assert.ok(joined.rgba[16]>20&&joined.rgba[16]<90);
});

test('common luminance treatment is opt-in and preserves alpha',()=>{
  const rgba=Uint8Array.from([15,80,99,255,0,0,0,0]);
  assert.equal(quantizeImage(rgba,0),rgba);
  const result=quantizeImage(rgba,4);
  assert.equal(result[3],255);assert.equal(result[7],0);assert.notDeepEqual(result,rgba);
});

test('all sample rates share one sampled time; static clips and wrap are defined',()=>{
  for(let fps=2;fps<=60;fps++) for(const phase of [0,.237,.5,.75,.999]) {
    assert.equal(sampleTime(phase,1,fps),Math.floor(phase*fps)/fps);
  }
  assert.equal(sampleTime(20,0,60),0);assert.equal(sampleTime(1,1,60),0);
  assert.ok(sampleTime(1-1e-12,1,60)<1);assert.ok(sampleTime(.017-1e-12,.017,60)<.017);
  assert.equal(advanceTime(.25,10,1),.35);
});
