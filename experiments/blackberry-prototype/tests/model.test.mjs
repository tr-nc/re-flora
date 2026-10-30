import test from 'node:test';
import assert from 'node:assert/strict';
import {makePlant,point} from '../model.mjs';

test('a seed reproduces an entire specimen; another seed changes its silhouette',()=>{
  assert.deepEqual(makePlant(),makePlant());
  assert.notDeepEqual(makePlant({seed:27}).canes,makePlant({seed:44}).canes);
});
test('density edits preserve the skeleton and fruit-bearing sites',()=>{
  const sparse=makePlant({density:.2}),full=makePlant({density:1});
  assert.deepEqual(sparse.canes,full.canes);
  assert.deepEqual(sparse.fruitSites,full.fruitSites);
  assert.ok(sparse.leaves.length<full.leaves.length);
  for(const leaf of sparse.leaves)assert.ok(full.leaves.some(other=>JSON.stringify(other)===JSON.stringify(leaf)));
});
test('display stages cannot alter the morphological model',()=>{
  assert.deepEqual(makePlant({stage:'fruit'}),makePlant({stage:'flower'}));
  assert.deepEqual(makePlant({skeleton:true}),makePlant({skeleton:false}));
});
test('growth has multiple ground-fixed roots, both cane generations, and arching tips',()=>{
  const plant=makePlant();
  assert.equal(plant.canes.length,11);
  assert.ok(plant.canes.some(cane=>cane.fresh));
  assert.ok(plant.canes.some(cane=>!cane.fresh));
  for(const cane of plant.canes) {
    assert.equal(point(cane.curve,0)[1],0);
    if(!cane.fresh) assert.ok(point(cane.curve,.6)[1]>point(cane.curve,1)[1]);
  }
});
test('fruit sites terminate attached laterals on older canes only',()=>{
  const plant=makePlant();
  assert.ok(plant.fruitSites.length>0);
  for(const site of plant.fruitSites) {
    const parent=plant.canes.find(cane=>cane.id===site.cane);
    assert.equal(parent.fresh,false);
    assert.ok(parent.laterals.some(branch=>JSON.stringify(point(branch.curve,1))===JSON.stringify(site.base)));
  }
});
test('spread edits change lateral reach without moving roots or changing attachment count',()=>{
  const narrow=makePlant({spread:.65}),wide=makePlant({spread:1.4});
  assert.equal(narrow.leaves.length,wide.leaves.length);
  for(let i=0;i<narrow.canes.length;i++) {
    const a=narrow.canes[i].curve,b=wide.canes[i].curve;
    assert.deepEqual(a[0],b[0]);
    assert.equal(a[3][1],b[3][1]);
    assert.ok(Math.hypot(b[3][0]-b[0][0],b[3][2]-b[0][2])>Math.hypot(a[3][0]-a[0][0],a[3][2]-a[0][2]));
  }
});
test('supported parameter extremes produce finite, above-ground geometry',()=>{
  for(const seed of [0,27,44,99999])for(const canes of [5,17])for(const spread of [.65,1.4]) {
    const plant=makePlant({seed,canes,spread});
    assert.equal(plant.canes.length,canes);
    for(const cane of plant.canes)for(const curve of [cane.curve,...cane.laterals.map(b=>b.curve)])for(let i=0;i<=20;i++) {
      const p=point(curve,i/20);assert.ok(p.every(Number.isFinite));assert.ok(p[1]>=0);
    }
    for(const item of [...plant.leaves,...plant.fruitSites])assert.ok(item.base.every(Number.isFinite));
  }
});
test('invalid input is normalized at the model boundary',()=>{
  const plant=makePlant({seed:NaN,canes:1000,spread:-10,density:Infinity});
  assert.deepEqual(plant.settings,{seed:27,canes:17,spread:.65,density:.78});
});
