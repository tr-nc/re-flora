import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog as nativeCatalog,flowerGeometry as nativeGeometry} from '../../../assets/models/flower-source.mjs';
import {flowerCatalog,flowerGeometry,previewOnlyFlowers} from '../models/flower-catalog.mjs';

test('two browser-only white flowers leave all native definitions and geometry unchanged',()=>{
  assert.equal(nativeCatalog.length,8);
  assert.equal(flowerCatalog.length,10);
  assert.equal(new Set(flowerCatalog.map(spec=>spec.id)).size,10);
  assert.deepEqual(previewOnlyFlowers.map(spec=>spec.id),['white-geranium','star-strawberry']);
  for(const native of nativeCatalog){
    assert.equal(flowerCatalog.find(spec=>spec.id===native.id),native);
    assert.deepEqual(flowerGeometry(native.id),nativeGeometry(native.id));
  }
  assert.deepEqual(flowerGeometry('white-geranium'),nativeGeometry('wild-geranium'));
  assert.notDeepEqual(flowerGeometry('star-strawberry').parts,flowerGeometry('white-geranium').parts);
});
test('preview-only flowers preserve complete head groups and working shape controls',()=>{
  for(const spec of previewOnlyFlowers){
    const original=flowerGeometry(spec.id);
    assert.deepEqual(flowerGeometry(spec.id),original);
    assert.equal(original.heads.length,spec.heads.length);
    assert.equal(original.parts.filter(part=>part.head===null).length,2);
    for(const head of original.heads){
      assert.deepEqual(original.parts.filter(part=>part.head===head.id).map(part=>part.material),['petalColor','innerColor','centerColor','stemColor']);
      assert.ok(head.anchor.every(Number.isFinite));
    }
    for(const recipe of [original,...Object.entries({height:.75,flowerSize:1.3,opening:.6,tilt:85,leafSize:1.35,bend:-.25}).map(([key,value])=>{
      const altered=flowerGeometry(spec.id,{[key]:value});
      assert.notDeepEqual(altered,original,`${spec.id}: ${key}`);return altered;
    })])for(const part of recipe.parts){
      assert.ok(part.positions.length>0&&part.positions.every(Number.isFinite));
      assert.equal(part.positions.length%3,0);assert.equal(part.indices.length%3,0);
      assert.ok(part.indices.length>0&&part.indices.every(i=>Number.isInteger(i)&&i>=0&&i<part.positions.length/3));
      assert.match(spec.defaults[part.material],/^#[a-f0-9]{6}$/i);
    }
    assert.deepEqual(flowerGeometry(spec.id,{petalColor:'#ffffff'}),original);
    const rgb=spec.defaults.petalColor.slice(1).match(/../g).map(n=>parseInt(n,16));
    assert.ok(rgb.every(n=>n>=230),'warm white petals');
  }
});
