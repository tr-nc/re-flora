import test from 'node:test';
import assert from 'node:assert/strict';
import {rasterTriangleVariants,rasterDepthAt} from './raster-depth-oracle.mjs';

test('stable raster interiors retain exact depth checks; top-left boundaries are explicit',()=>{
  const triangles=[rasterTriangleVariants([[0,0,-.6],[8,0,-.6],[0,8,-.6]],64,16)];
  assert.equal(triangles[0].length,1);
  const hit=rasterDepthAt(triangles,2.5,2.5);
  assert.ok(Math.abs(hit.depth-.2)<1e-12);
  assert.equal(hit.boundary,false);assert.equal(hit.ambiguous,false);
  assert.equal(rasterDepthAt(triangles,0,0).boundary,true);
  assert.equal(rasterDepthAt(triangles,9,9).depth,Infinity);
});

test('float32 projection ties can change triangle ownership without invalid GPU depth',()=>{
  // Actual 64px tulip side-view capture. The shared vertex's X is only 1.6e-6
  // above the 45.15625 tie. GPU rounding chose the lower subpixel coordinate.
  const shared=[45.15625158507832,26.88366907529151,-.6976787266594836];
  const upper=[46.073842420988655,28.557534653400698,-.7013255857923875];
  const triangles=[
    [[44.952587707625554,25.099703807103655,-.7002640989278406],upper,shared],
    [upper,[46.03442634513641,29.72828159382933,-.6974493077621728],shared],
  ].map(t=>rasterTriangleVariants(t,64,16));
  assert.ok(triangles.every(v=>v.length===2));
  const hit=rasterDepthAt(triangles,45.5,27.5);
  assert.equal(hit.ambiguous,true);
  assert.ok(Math.abs(hit.depth-.15037256479263306)<1e-6,'GPU depth matches a valid snap, not a widened depth tolerance');
  const untouched=rasterDepthAt(triangles,10.5,10.5);
  assert.equal(untouched.ambiguous,false,'uncertainty is local, never a whole-tile waiver');
  assert.equal(untouched.depth,Infinity);
  const foreground=rasterTriangleVariants([[40,20,-.9],[60,20,-.9],[40,40,-.9]],64,16);
  const occluded=rasterDepthAt([...triangles,foreground],45.5,27.5);
  assert.equal(occluded.ambiguous,false,'an uncertain surface behind a stable nearer hit does not waive its depth check');
  assert.ok(Math.abs(occluded.depth-.05)<1e-12);
});

test('the snapping ambiguity bound scales with resolution and does not absorb clear non-ties',()=>{
  for(const size of [8,32,64,128])for(const scale of [16,256]){
    const tie=2+.5/scale;
    for(const delta of [-1e-7,0,1e-7]){
      assert.equal(rasterTriangleVariants([[tie+delta,1,0],[4,1,0],[4,4,0]],size,scale).length,2);
    }
    assert.equal(rasterTriangleVariants([[tie+.1/scale,1,0],[4,1,0],[4,4,0]],size,scale).length,1);
  }
});
