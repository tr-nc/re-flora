import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog,flowerGeometry} from '../../../assets/models/flower-source.mjs';
import {readFile} from 'node:fs/promises';
import {publishedFlowers} from '../../../scripts/publish-flower-models.mjs';

test('native flower publication regenerates byte-for-byte from the browser recipe',async()=>{
  assert.equal(await readFile(new URL('../../../assets/models/flowers.json',import.meta.url),'utf8'),await publishedFlowers());
});

test('eight distinct original flower recipes have finite, low-poly geometry and complete head groups',()=>{
  assert.equal(flowerCatalog.length,8);
  const shapes=new Set();
  for(const spec of flowerCatalog){
    const recipe=flowerGeometry(spec.id);
    assert.deepEqual(flowerGeometry(spec.id),recipe,'deterministic authoring');
    shapes.add(JSON.stringify(recipe.parts.map(part=>part.positions)));
    let triangles=0;
    for(const part of recipe.parts){
      assert.ok(part.positions.every(Number.isFinite));
      assert.ok(part.indices.every(i=>Number.isInteger(i)&&i>=0&&i<part.positions.length/3));
      assert.equal(part.indices.length%3,0);
      assert.match(spec.defaults[part.material],/^#[a-f0-9]{6}$/i);
      triangles+=part.indices.length/3;
    }
    assert.ok(triangles>100&&triangles<1600,`${spec.id}: ${triangles} triangles`);
    assert.equal(recipe.heads.length,spec.heads.length);
    for(const head of recipe.heads){
      const parts=recipe.parts.filter(part=>part.head===head.id);
      assert.deepEqual(parts.map(part=>part.material),['petalColor','innerColor','centerColor','stemColor']);
      assert.ok(parts.every(part=>part.indices.length>0));
      assert.ok(head.anchor.every(Number.isFinite));
    }
    assert.equal(recipe.parts.filter(part=>part.head===null).length,2,'stems and leaves stay outside flower tiles');
  }
  assert.equal(shapes.size,8,'not eight recolors of the same mesh');
});

test('all shape sliders change geometry without invalid vertices, color-only edits do not',()=>{
  for(const spec of flowerCatalog){
    const original=flowerGeometry(spec.id);
    for(const [key,values]of Object.entries({height:[.75,1.15],flowerSize:[.65,1.3],opening:[.6,1.35],tilt:[-15,85],leafSize:[.6,1.35],bend:[-.25,.25]})){
      for(const value of values){
        const altered=flowerGeometry(spec.id,{[key]:value});
        assert.notDeepEqual(altered.parts,original.parts,`${spec.id} ${key} has an effect`);
        assert.ok(altered.parts.every(part=>part.positions.every(Number.isFinite)));
      }
    }
    assert.deepEqual(flowerGeometry(spec.id,{petalColor:'#ffffff'}),original);
  }
});
