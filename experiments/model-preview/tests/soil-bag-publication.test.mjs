import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {publishedSoilBag,soilBagSnapshots,soilBagStickerSvg} from '../../../scripts/publish-soil-bag-model.mjs';
import {soilBagPose} from '../models/soil-bag-geometry.mjs';

const asset=new URL('../../../assets/models/soil-bag.json',import.meta.url);
test('native soil-bag snapshots are deterministic complete shared geometry, not animation approximations',async()=>{
  const png=await readFile(new URL('../../../assets/models/soil-bag-sticker.png',import.meta.url));
  assert.equal(await readFile(asset,'utf8'),await publishedSoilBag(crc32(png)));
  assert.deepEqual(soilBagSnapshots(),soilBagSnapshots());
  const states=soilBagSnapshots();assert.equal(states.length,2);
  assert.deepEqual(states.map(s=>s.angle),[0,soilBagPose(1.9,1).angle]);
  for(const state of states){
    assert.equal(state.parts.length,6);
    for(const part of state.parts){
      assert.equal(part.normals.length,part.positions.length);assert.equal(part.uvs.length,part.positions.length/3*2);
      assert.ok(part.positions.every(Number.isFinite));assert.ok(part.normals.every(Number.isFinite));
      assert.ok(part.indices.every(i=>i>=0&&i<part.positions.length/3));
    }
  }
  assert.notDeepEqual(states[0].parts[0].positions,states[1].parts[0].positions,'pour snapshot has the real opened mouth');
  assert.ok(Math.cos(states[1].angle)<-.7,'the published pouring pose points down immediately');
});
test('published soil-bag sticker retains original continuous text-free vector source',()=>{
  const svg=soilBagStickerSvg();assert.match(svg,/width="512" height="640"/);assert.match(svg,/<path[^>]+C/);
  assert.doesNotMatch(svg,/<text|<image/);assert.equal((svg.match(/<rect/g)||[]).length,2);
});
