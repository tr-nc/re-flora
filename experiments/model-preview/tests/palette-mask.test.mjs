import test from 'node:test';
import assert from 'node:assert/strict';
import {weightsFromRGB,createMask,maskPresets,resolvePalette,paintMask,validateMask} from '../../../assets/models/palette-mask.mjs';

test('weights represent palette slots, not interpolated numeric indices',()=>{
  assert.deepEqual(weightsFromRGB([255,0,0]),[1,0,0,0]);
  assert.deepEqual(weightsFromRGB([0,255,0]),[0,1,0,0]);
  assert.deepEqual(weightsFromRGB([0,0,255]),[0,0,1,0]);
  assert.deepEqual(weightsFromRGB([0,0,0]),[0,0,0,1]);
  assert.deepEqual(weightsFromRGB([255,0,255]),[.5,0,.5,0]);
  assert.deepEqual(weightsFromRGB([255,255,255]),[1/3,1/3,1/3,0]);
  for(const rgb of [[42,27,19],[128,128,0],[255,170,73]])assert.ok(Math.abs(weightsFromRGB(rgb).reduce((a,b)=>a+b,0)-1)<1e-12);
});
test('palette blends in linear light and slots can be swapped without editing weights',()=>{
  const mask=createMask('solid',32);mask.pixels.fill(255);
  for(let i=0;i<mask.pixels.length;i+=4){mask.pixels[i]=128;mask.pixels[i+1]=128;mask.pixels[i+2]=0;}
  const before=mask.pixels.slice(),palette=['#0000ff','#ffffff','#ff0000','#000000'];
  const color=resolvePalette(mask,palette);
  assert.deepEqual([...color.slice(0,4)],[188,188,255,255]);
  assert.deepEqual(resolvePalette(mask,[palette[1],palette[0],palette[2],palette[3]]),color);
  assert.deepEqual(mask.pixels,before);
});
test('templates, soft brush, black fourth slot and validation are deterministic',()=>{
  for(const [id]of maskPresets){const m=createMask(id);assert.deepEqual(createMask(id),m);validateMask(m);}
  const mask=createMask('solid',32),before=mask.pixels.slice();
  const painted=paintMask(mask,8.5,8.5,3,4);
  assert.deepEqual([...painted.pixels.slice((8*32+8)*4,(8*32+8)*4+4)],[0,0,0,255]);
  assert.deepEqual(mask.pixels,before);
  assert.ok(painted.pixels.some((v,i)=>i%4===0&&v>0&&v<255));
  assert.throws(()=>validateMask({...mask,pixels:new Uint8Array(mask.pixels.length)}),/不透明/);
  assert.throws(()=>createMask('unknown'));
  assert.throws(()=>validateMask({...mask,width:100000}));
  assert.throws(()=>resolvePalette(mask,['red']));
});
