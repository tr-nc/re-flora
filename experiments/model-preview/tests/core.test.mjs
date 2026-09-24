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

test('a visible subpixel leaf stem survives even when no center pixel was sampled',()=>{
  const size=8,rgba=new Uint8Array(size*size*4),owners=new Uint32Array(size*size);
  const stem=[[[3.98,1.2,.2],[4.02,1.2,.2],[3.98,3.6,.2]],[[4.02,1.2,.2],[4.02,3.6,.2],[3.98,3.6,.2]]];
  const result=repairImage(rgba,owners,[{id:1,triangles:stem,preserve:[{triangles:stem,color:[110,75,30]}]}],size);
  assert.ok(result.added>0);
  assert.ok(Array.from({length:size*size},(_,i)=>result.rgba[i*4+3]).some(Boolean));
  assert.deepEqual(result.rgba.subarray(1*size*4+4*4,1*size*4+4*4+4),Uint8Array.from([110,75,30,255]));
  rgba.set([7,8,9,255],(size+4)*4);owners[size+4]=2;
  const occluded=repairImage(rgba,owners,[{id:1,triangles:stem,preserve:[{triangles:stem,color:[110,75,30]}]}],size);
  assert.deepEqual(occluded.rgba.subarray((size+4)*4,(size+5)*4),Uint8Array.from([7,8,9,255]));
  rgba.fill(0);owners.fill(0);rgba.set([2,3,4,255],(size+4)*4);owners[size+4]=1;
  const sampled=repairImage(rgba,owners,[{id:1,triangles:stem,preserve:[{triangles:stem,color:[110,75,30]}]}],size);
  assert.deepEqual(sampled.rgba.subarray((size+4)*4,(size+5)*4),Uint8Array.from([2,3,4,255]));
  assert.ok(sampled.added>0,'preserve the rest of a stem even when one center pixel was sampled');
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
