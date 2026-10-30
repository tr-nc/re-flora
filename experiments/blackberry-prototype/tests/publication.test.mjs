import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from '../../butterfly-method-comparison/vendor/three-0.183.0/build/three.module.js';
import {blackberryMesh,blackberryColors} from '../../../assets/models/blackberry-source.mjs';
import {publishedBlackberry,BLACKBERRY_AUTHORING_SCALE} from '../../../scripts/blackberry-publication.mjs';

test('native blackberry preserves every approved browser surface, not a replacement flower crown',()=>{
  const mesh=blackberryMesh(THREE),native=publishedBlackberry();
  assert.equal(native.kind,'shrub');assert.equal(native.stem_layers,0);
  assert.deepEqual(native.heads,[{id:0,anchor:[0,0,0]}]);
  assert.equal(native.parts.length,mesh.parts.length);
  for(let i=0;i<mesh.parts.length;i++) {
    const web=mesh.parts[i],part=native.parts[i];
    assert.equal(part.indices.length,web.positions.length/3);
    for(let vertex=0;vertex<part.indices.length;vertex++)for(let axis=0;axis<3;axis++) {
      assert.equal(part.positions[part.indices[vertex]*3+axis],web.positions[vertex*3+axis]*BLACKBERRY_AUTHORING_SCALE);
    }
    const uv=part.uvs.slice(0,2),x=Math.floor(uv[0]*32),y=Math.floor(uv[1]*32);
    const sampled=native.color_texture.rgb.slice((y*32+x)*3,(y*32+x)*3+3);
    const expected=[1,3,5].map(c=>parseInt(blackberryColors[web.material].slice(c,c+2),16));
    assert.deepEqual(sampled,expected,`${web.material} atlas swatch`);
  }
  assert.ok(mesh.parts.some(p=>p.material==='old'));
  assert.ok(mesh.parts.some(p=>p.material==='fresh'));
  assert.ok(mesh.parts.some(p=>p.material.startsWith('leaf')));
  assert.ok(mesh.parts.some(p=>p.material==='ripe'));
  assert.ok(!mesh.parts.some(p=>p.material==='petal'),'fruiting shrub is not an invented nectar source');
});
