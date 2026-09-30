import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog,flowerGeometry} from '../../../assets/models/flower-source.mjs';
import {maskValue,resolvePalette} from '../../../assets/models/palette-mask.mjs';
import {readFile} from 'node:fs/promises';
import {publishedFlowers} from '../../../scripts/publish-flower-models.mjs';

test('native flower publication regenerates byte-for-byte from shared geometry and palette texture',async()=>{
  const json=await publishedFlowers();
  assert.equal(await readFile(new URL('../../../assets/models/flowers.json',import.meta.url),'utf8'),json);
  const {flowers}=JSON.parse(json);
  assert.deepEqual(flowers.map(f=>[f.id,f.stem_layers]),[['wild-geranium',41],['forget-me-not',42],['oxeye-daisy',42],['cosmos',42],['coneflower',43],['tulip',37]]);
  for(const [i,f]of flowers.entries()){
    const spec=flowerCatalog[i],recipe=flowerGeometry(spec.id);
    for(const [j,part]of f.parts.entries()){
      assert.deepEqual(part.indices,recipe.parts[j].indices);
      assert.deepEqual(part.positions,recipe.parts[j].positions.map(v=>Number(v.toFixed(9))||0));
      assert.deepEqual(part.uvs,recipe.parts[j].uvs.map(v=>Number(v.toFixed(9))||0));
    }
    const mask=maskValue(spec.defaults.weightMap),rgba=resolvePalette(mask,['A','B','C','D'].map(k=>spec.defaults['palette'+k]));
    assert.equal(f.color_texture.width,mask.width);assert.equal(f.color_texture.height,mask.height);
    assert.deepEqual(f.color_texture.rgb,Array.from(rgba).filter((_,i)=>i%4!==3));
  }
});
test('published species are distinct presets with one complete head and no legacy plant branches',()=>{
  const shapes=new Set();
  for(const spec of flowerCatalog){
    const recipe=flowerGeometry(spec.id);assert.deepEqual(flowerGeometry(spec.id),recipe);
    shapes.add(JSON.stringify(recipe.parts.map(p=>p.positions)));
    let count=0;
    for(const part of recipe.parts){
      assert.equal(part.material,'palette');assert.equal(part.head,0);
      assert.ok(part.positions.every(Number.isFinite));assert.equal(part.uvs.length,part.positions.length/3*2);
      assert.ok(part.indices.every(i=>Number.isInteger(i)&&i>=0&&i<part.positions.length/3));
      count+=part.indices.length/3;
    }
    assert.ok(count>100&&count<3000);assert.equal(recipe.heads.length,1);
    assert.deepEqual(recipe.parts.map(p=>p.name),['Flower petals','Flower center','Flower calyx']);
    assert.equal(spec.kind,undefined);assert.equal(spec.cacheTemplate,undefined);
    assert.equal(spec.heads,undefined,'old whole-plant flower recipes removed');
  }
  assert.equal(shapes.size,flowerCatalog.length);
});
test('shared geometry responds only to shape settings, never palette or weight-map edits',()=>{
  for(const spec of flowerCatalog){
    const original=flowerGeometry(spec.id);
    for(const [key,value]of Object.entries({petalCount:3,flowerSize:.65,opening:-1,tilt:-15,petalLength:1.4,petalWidth:.08})){
      const changed=flowerGeometry(spec.id,{[key]:value});assert.notDeepEqual(changed,original);
      assert.ok(changed.parts.every(part=>part.positions.every(Number.isFinite)));
    }
    assert.deepEqual(flowerGeometry(spec.id,{paletteA:'#ffffff',weightMap:'blue-white'}),original);
  }
});
