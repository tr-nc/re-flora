import test from 'node:test';
import assert from 'node:assert/strict';
import {animalCatalog} from '../models/animal-geometry.mjs';
import {createKingfisherPlumage,resolveKingfisherPlumage,kingfisherColorKeys} from '../models/kingfisher-plumage.mjs';
const palette=animalCatalog.find(spec=>spec.id==='kingfisher').colors;

test('original plumage is deterministic, opaque and has normalized finite seven-slot palette weights',()=>{
  const maps=createKingfisherPlumage(32);assert.deepEqual(createKingfisherPlumage(32),maps);
  assert.deepEqual(Object.keys(maps),['head','body','wing','tail','bill']);
  const used=Array(7).fill(0);
  for(const mask of Object.values(maps)){
    assert.equal(mask.weights.length,32*32*7);assert.equal(mask.shades.length,32*32);
    assert.ok(mask.shades.every(v=>Number.isFinite(v)&&v>0&&v<=1.1));
    for(let i=0;i<32*32;i++){
      const weights=Array.from(mask.weights.slice(i*7,i*7+7));
      assert.ok(weights.every(w=>Number.isFinite(w)&&w>=0&&w<=1));
      assert.ok(Math.abs(weights.reduce((a,b)=>a+b,0)-1)<1e-6);
      weights.forEach((w,k)=>used[k]+=w);
    }
    const pixels=resolveKingfisherPlumage(mask,palette);
    assert.ok(pixels.every((v,i)=>i%4!==3||v===255),'plumage must not introduce alpha coverage / cutouts');
  }
  assert.ok(used.every(value=>value>0),'all seven color controls have authored regions');
});

test('plumage palette updates reuse pixel storage without changing UV weights or height/detail masks',()=>{
  const maps=createKingfisherPlumage(32),original=createKingfisherPlumage(32);
  const changed=Object.fromEntries(kingfisherColorKeys.map(key=>[key,'#31c29e']));
  for(const mask of Object.values(maps)){
    const pixels=resolveKingfisherPlumage(mask,palette),before=pixels.slice();
    assert.equal(resolveKingfisherPlumage(mask,changed,pixels),pixels);
    assert.notDeepEqual(pixels,before);
    resolveKingfisherPlumage(mask,palette,pixels);assert.deepEqual(pixels,before);
  }
  assert.deepEqual(maps,original);
  assert.throws(()=>createKingfisherPlumage(0),/size/);
  assert.throws(()=>resolveKingfisherPlumage(maps.head,{...palette,bodyColor:'bad'}),/palette/);
});
