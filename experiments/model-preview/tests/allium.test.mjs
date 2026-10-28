import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog,flowerGeometry} from '../../../assets/models/flower-source.mjs';
import {flowerCatalog as studio} from '../models/flower-catalog.mjs';
const points=part=>Array.from({length:part.positions.length/3},(_,i)=>part.positions.slice(i*3,i*3+3));

test('allium is an appended ornamental umbel, not a root crop or a substituted radial studio flower',()=>{
  const index=flowerCatalog.findIndex(s=>s.id==='allium'),spec=flowerCatalog[index];
  assert.equal(flowerCatalog[index-1].id,'carrot');
  assert.equal(spec.morphology,'spherical_umbel');assert.notEqual(spec.kind,'root_crop');
  assert.equal(spec.displayName,'Allium');assert.equal(spec.stemLayers,68);
  assert.ok(!studio.some(s=>s.id==='allium'),'single-whorl controls cannot edit a compound umbel');
});

test('allium has a three-dimensional globe of six-point stars, connected pedicels and a closed stem socket',()=>{
  const model=flowerGeometry('allium');
  assert.deepEqual(model,flowerGeometry('allium'));
  assert.deepEqual(model.socketNormal,[0,1,0]);
  assert.deepEqual(model.heads[0].anchor,[0,0,0]);
  const [florets,pedicels,socket]=model.parts;
  const stars=points(florets),bounds=[0,1,2].map(axis=>{
    const values=stars.map(p=>p[axis]);return [Math.min(...values),Math.max(...values)];
  });
  const spans=bounds.map(([lo,hi])=>hi-lo);
  assert.ok(Math.max(...spans)/Math.min(...spans)<1.1,'spherical from every axis, not a disk');
  assert.ok(bounds[1][0]>0,'umbel does not bury the native attachment');
  // Each floret center lies at the end of a pedicel, on the spherical shell.
  const centers=stars.filter((_,i)=>i%19===0);
  assert.equal(centers.length,120);assert.equal(florets.indices.length/3,centers.length*12);
  for(const center of centers){
    assert.ok(Math.abs(Math.hypot(center[0],center[1]-.72,center[2])-.62)<1e-10);
    assert.ok(points(pedicels).some(p=>Math.hypot(...p.map((v,i)=>v-center[i]))<.01));
  }
  assert.equal(Math.min(...points(socket).map(p=>p[1])),0);
  assert.ok(Math.max(...points(socket).map(p=>p[1]))>.72,'receptacle reaches the shared pedicel hub');
  for(const part of model.parts){
    assert.ok(part.positions.every(Number.isFinite));
    assert.equal(part.uvs.length,part.positions.length/3*2);
    assert.ok(part.uvs.every(v=>v>=0&&v<=1));
    assert.ok(part.indices.every(i=>Number.isInteger(i)&&i>=0&&i<part.positions.length/3));
    const ps=points(part);
    for(let i=0;i<part.indices.length;i+=3){
      const [a,b,c]=part.indices.slice(i,i+3).map(index=>ps[index]);
      const u=b.map((v,j)=>v-a[j]),v=c.map((n,j)=>n-a[j]);
      const area=Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]);
      assert.ok(area>1e-8,'no degenerate triangles');
    }
  }
});
