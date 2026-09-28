import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog,flowerGeometry} from '../models/flower-catalog.mjs';
import {flowerCatalog as nativeCatalog} from '../../../assets/models/flower-source.mjs';
import {completeFlowerHead} from '../../../assets/models/flower-head.mjs';
import {publishedFlowers} from '../../../scripts/publish-flower-models.mjs';

test('all preview models contain exactly one complete attachment-local head, no stalk or leaves',()=>{
  for(const spec of flowerCatalog){
    const authored=flowerGeometry(spec.id),before=JSON.stringify(authored);
    const model=completeFlowerHead(authored),head=authored.heads[0];
    assert.equal(JSON.stringify(authored),before,'authoring source is immutable');
    assert.deepEqual(model.heads,[{id:0,anchor:[0,0,0],label:'完整花头'}]);
    assert.equal(model.column,undefined);
    const expected=authored.parts.filter(part=>part.head===head.id);
    assert.equal(model.parts.length,expected.length);
    assert.ok(model.parts.some(part=>part.name.includes('calyx')),'calyx belongs to flower');
    model.parts.forEach((part,i)=>{
      assert.equal(part.head,0);assert.notEqual(part.material,'leafColor');
      assert.deepEqual(part.indices,expected[i].indices);
      assert.deepEqual(part.positions,expected[i].positions.map((v,k)=>v-head.anchor[k%3]));
      assert.notEqual(part.positions,expected[i].positions);
    });
  }
});
test('native publication contains one head per catalog flower, without assembly, leaves or stems',async()=>{
  const published=JSON.parse(await publishedFlowers());
  assert.equal(published.flowers.length,nativeCatalog.length);
  for(const flower of published.flowers){
    assert.equal(flower.column,undefined);assert.equal(flower.root,undefined);
    assert.equal(flower.palette.length,4);assert.ok(flower.cache_family);
    assert.deepEqual(flower.heads[0].anchor,[0,0,0]);
    assert.ok(flower.parts.every(part=>part.head===0&&part.material!=='leafColor'));
  }
});
test('shared family publishes identical geometry but distinct species palettes',async()=>{
  const {flowers}=JSON.parse(await publishedFlowers());
  const forget=flowers.find(f=>f.id==='forget-me-not'),cosmos=flowers.find(f=>f.id==='cosmos');
  assert.equal(forget.cache_family,cosmos.cache_family);
  assert.notDeepEqual(forget.palette,cosmos.palette);
  assert.deepEqual(forget.parts.map(({color,...part})=>part),cosmos.parts.map(({color,...part})=>part));
  assert.equal(flowers.some(f=>['corn-poppy','bellflower'].includes(f.id)),false);
});
test('missing flower heads are rejected',()=>assert.throws(()=>completeFlowerHead({parts:[],heads:[]})));
