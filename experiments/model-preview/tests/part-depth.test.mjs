import test from 'node:test';
import assert from 'node:assert/strict';
import {tileDepth,unpackDepth} from '../part-depth.mjs';

test('depth unpacking matches vendored Three r183 RGBADepthPacking',()=>{
  assert.equal(unpackDepth([0,0,0,0],0),0);
  assert.equal(unpackDepth([128,0,0,0],0),.5);
  assert.equal(unpackDepth([255,255,255,255],0),1);
  assert.equal(unpackDepth([0,1,0,0],0),1/65536);
});
test('composite uses original GPU depth, geometric repair depth and transparent background',()=>{
  const original=new Uint8Array(16),rgba=new Uint8Array(16),packed=new Uint8Array(16);
  original[3]=rgba[3]=rgba[7]=255;packed[0]=192;
  const groups=[{triangles:[[[0,0,-.4],[2,0,-.4],[2,2,-.4]]]}];
  const depth=tileDepth(original,rgba,packed,groups,2);
  assert.equal(depth[0],.75,'original geometry depth is never replaced with a billboard plane');
  assert.ok(Math.abs(depth[1]-.3)<1e-6);
  assert.equal(depth[2],1);assert.equal(depth[3],1);
  assert.throws(()=>tileDepth(original,rgba,packed,[],2),/no geometric depth/);
});
