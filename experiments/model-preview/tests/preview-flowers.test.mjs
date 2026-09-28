import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog as nativeCatalog,flowerGeometry as nativeGeometry} from '../../../assets/models/flower-source.mjs';
import {flowerCatalog,flowerGeometry,previewOnlyFlowers} from '../models/flower-catalog.mjs';

test('two browser-only white flowers leave all native definitions and geometry unchanged',()=>{
  assert.equal(nativeCatalog.length,8);
  assert.equal(flowerCatalog.length,10);
  assert.equal(new Set(flowerCatalog.map(spec=>spec.id)).size,10);
  assert.deepEqual(previewOnlyFlowers.map(spec=>spec.id),['white-geranium','gillenia']);
  for(const native of nativeCatalog){
    assert.equal(flowerCatalog.find(spec=>spec.id===native.id),native);
    assert.deepEqual(flowerGeometry(native.id),nativeGeometry(native.id));
  }
  assert.deepEqual(flowerGeometry('white-geranium'),nativeGeometry('wild-geranium'));
  assert.notDeepEqual(flowerGeometry('gillenia').parts,flowerGeometry('white-geranium').parts);
});
test('Gillenia has separated narrow petals, small centers and a loose multi-flower silhouette',()=>{
  const spec=previewOnlyFlowers.find(spec=>spec.id==='gillenia'),recipe=flowerGeometry(spec.id);
  assert.equal(spec.label,'星草梅');assert.equal(spec.latin,'Gillenia trifoliata');
  assert.equal(recipe.heads.length,7);
  for(const head of recipe.heads){
    const petals=recipe.parts.find(part=>part.head===head.id&&part.material==='petalColor');
    assert.equal(petals.positions.length,5*9*3,'five independent eight-sided petal rims');
    const radius=spec.heads[head.id][3];
    for(let petal=0;petal<5;petal++){
      const points=Array.from({length:9},(_,i)=>petals.positions.slice((petal*9+i)*3,(petal*9+i+1)*3).map((n,k)=>n-head.anchor[k]));
      const tip=points.reduce((a,b)=>Math.hypot(...a)>Math.hypot(...b)?a:b),length=Math.hypot(...tip),axis=tip.map(n=>n/length);
      assert.ok(length>.85*radius,'long pointed petal');
      for(const point of points){
        const along=point.reduce((sum,n,k)=>sum+n*axis[k],0);
        assert.ok(Math.hypot(...point.map((n,k)=>n-along*axis[k]))<length*.24,'narrow, not rounded strawberry lobes');
      }
    }
    const center=recipe.parts.find(part=>part.head===head.id&&part.material==='centerColor');
    for(let i=0;i<center.positions.length;i+=3){
      assert.ok(Math.hypot(...center.positions.slice(i,i+3).map((n,k)=>n-head.anchor[k]))<radius*.1,'small cream center, no large golden disk');
    }
  }
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
