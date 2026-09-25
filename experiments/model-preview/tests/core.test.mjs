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

test('different repair groups preserve their existing colors and never overwrite center samples',()=>{
  const rgba=new Uint8Array(3*3*4),owners=new Uint32Array(9);
  rgba.set([20,50,80,255],0);rgba.set([90,40,10,255],32);owners[0]=1;owners[8]=2;
  const triangles=[[[0,0,0],[3,0,0],[3,3,0]],[[0,0,0],[3,3,0],[0,3,0]]];
  const result=repairImage(rgba,owners,[{id:1,triangles},{id:2,triangles}],3);
  assert.ok(result.added>0);
  assert.deepEqual(result.rgba.subarray(0,4),rgba.subarray(0,4));
  assert.deepEqual(result.rgba.subarray(32,36),rgba.subarray(32,36));
});

test('unchecked A/B mode keeps eight-neighbor bridges but omits unsampled geometry',()=>{
  const size=8,rgba=new Uint8Array(size*size*4),owners=new Uint32Array(size*size);
  const missed=[[[3.98,1.2,.2],[4.02,1.2,.2],[3.98,3.6,.2]]];
  const group={id:1,triangles:missed,fallbackColor:[110,75,30]};
  const oldMode=()=>repairImage(rgba,owners,[group],size,()=>null,{conservativeCoverage:false});
  assert.equal(oldMode().added,0,'a feature with no center samples stays absent in old mode');
  assert.ok(repairImage(rgba,owners,[group],size).added>0);

  // Original center samples can still be connected along projected geometry.
  const full=[[[1,1,.2],[6,1,.2],[6,6,.2]],[[1,1,.2],[6,6,.2],[1,6,.2]]];
  rgba.set([20,30,40,255],(2*size+2)*4);owners[2*size+2]=1;
  rgba.set([80,90,100,255],(2*size+5)*4);owners[2*size+5]=1;
  const connected=repairImage(rgba,owners,[{...group,triangles:full}],size,()=>null,{conservativeCoverage:false});
  assert.ok(connected.added>0,'old mode still repairs broken eight-neighbor connectivity');
  assert.equal(connected.groups[0].after,1);
  assert.equal(connected.groups[0].preserved,0);
  assert.deepEqual(connected.rgba.subarray((2*size+2)*4,(2*size+2)*4+4),Uint8Array.from([20,30,40,255]));
});

test('all projected triangles survive missing center samples without per-feature annotations',()=>{
  const size=8,rgba=new Uint8Array(size*size*4),owners=new Uint32Array(size*size);
  const stem=[[[3.98,1.2,.2],[4.02,1.2,.2],[3.98,3.6,.2]],[[4.02,1.2,.2],[4.02,3.6,.2],[3.98,3.6,.2]]];
  const group={id:1,triangles:stem,fallbackColor:[110,75,30]};
  const result=repairImage(rgba,owners,[group],size);
  assert.ok(result.added>0);
  assert.ok(Array.from({length:size*size},(_,i)=>result.rgba[i*4+3]).some(Boolean));
  assert.deepEqual(result.rgba.subarray(1*size*4+4*4,1*size*4+4*4+4),Uint8Array.from([110,75,30,255]));
  rgba.set([7,8,9,255],(size+4)*4);owners[size+4]=2;
  const occluded=repairImage(rgba,owners,[group],size);
  assert.deepEqual(occluded.rgba.subarray((size+4)*4,(size+5)*4),Uint8Array.from([7,8,9,255]));
  rgba.fill(0);owners.fill(0);rgba.set([2,3,4,255],(size+4)*4);owners[size+4]=1;
  const sampled=repairImage(rgba,owners,[group],size);
  assert.deepEqual(sampled.rgba.subarray((size+4)*4,(size+5)*4),Uint8Array.from([2,3,4,255]));
  assert.ok(sampled.added>0,'preserve the rest of a stem even when one center pixel was sampled');
});

test('every visible projected footprint is represented, independent of model and center samples',()=>{
  const size=12,rgba=new Uint8Array(size*size*4),owners=new Uint32Array(size*size);
  for(let step=0;step<40;step++){
    const x=(step*7%10)+.1,y=(step*11%10)+.15;
    const triangles=[[[x,y,.3],[x+.025,y+.02,.3],[x+.01,y+1.4,.3]]];
    const coverage=projectedCoverage(triangles,size);
    const result=repairImage(rgba,owners,[{id:1,triangles,fallbackColor:[45,90,135]}],size);
    for(let i=0;i<size*size;i++)if(coverage.has(i)){
      assert.equal(result.rgba[i*4+3],255,`triangle ${step}, cell ${i}`);
    }
  }
});

test('nearer projected groups win empty pixels while existing samples remain untouched',()=>{
  const rgba=new Uint8Array(4*4*4),owners=new Uint32Array(16);
  rgba.set([1,2,3,255],0);owners[0]=1;
  const at=z=>[[[1,1,z],[3,1,z],[1,3,z]]];
  const output=repairImage(rgba,owners,[{id:1,triangles:at(.8),fallbackColor:[255,0,0]},{id:2,triangles:at(.2),fallbackColor:[0,0,255]}],4);
  assert.deepEqual(output.rgba.subarray(0,4),Uint8Array.from([1,2,3,255]));
  assert.deepEqual(output.rgba.subarray((1*4+1)*4,(1*4+1)*4+4),Uint8Array.from([0,0,255,255]));
});

test('material-driven shade palettes follow selected colors without introducing black',()=>{
  const rgba=Uint8Array.from([70,80,25,255,95,100,30,255,0,0,0,0]);
  assert.equal(quantizeImage(rgba,0),rgba);
  const green=[129,133,44],yellow=[210,163,52];
  for(const levels of [1,2,3,6]){
    const result=quantizeImage(rgba,levels,()=>green);
    assert.equal(result[3],255);assert.equal(result[7],255);assert.equal(result[11],0);
    assert.ok(result[0]>0&&result[1]>0&&result[4]>0&&result[5]>0,`level ${levels} turned leaf black`);
    if(levels===1)assert.deepEqual(Array.from(result.subarray(0,3)),green);
  }
  const changed=quantizeImage(rgba,1,()=>yellow);
  assert.deepEqual(Array.from(changed.subarray(0,3)),yellow);
});

test('all sample rates share one sampled time; static clips and wrap are defined',()=>{
  for(let fps=2;fps<=60;fps++) for(const phase of [0,.237,.5,.75,.999]) {
    assert.equal(sampleTime(phase,1,fps),Math.floor(phase*fps)/fps);
  }
  assert.equal(sampleTime(20,0,60),0);assert.equal(sampleTime(1,1,60),0);
  assert.ok(sampleTime(1-1e-12,1,60)<1);assert.ok(sampleTime(.017-1e-12,.017,60)<.017);
  assert.equal(advanceTime(.25,10,1),.35);
});
