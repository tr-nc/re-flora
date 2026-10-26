import test from 'node:test';
import assert from 'node:assert/strict';
import {carrotGeometry} from '../../../assets/models/carrot-source.mjs';
import {flowerCatalog,flowerGeometry} from '../../../assets/models/flower-source.mjs';
import {flowerCatalog as studio} from '../models/flower-catalog.mjs';

test('carrot has a buried taper and a connected feathery basal rosette, not a flower',()=>{
  const crop=carrotGeometry();
  assert.deepEqual(crop,carrotGeometry());
  assert.deepEqual(crop,flowerGeometry('carrot'));
  assert.equal(flowerCatalog.find(s=>s.id==='carrot').kind,'root_crop');
  assert.ok(!studio.some(s=>s.id==='carrot'),'head-only flower controls must not misrepresent crop geometry');
  const points=p=>Array.from({length:p.positions.length/3},(_,i)=>p.positions.slice(i*3,i*3+3));
  const root=points(crop.parts[0]),leaves=points(crop.parts[2]);
  assert.ok(Math.min(...root.map(p=>p[1]))<-.75);
  assert.ok(Math.max(...root.map(p=>p[1]))<.21,'only a shoulder is above soil');
  assert.ok(root.filter(p=>p[1]<-.6).every(p=>Math.hypot(p[0],p[2])<.06),'root tapers');
  assert.ok(Math.min(...leaves.map(p=>p[1]))<.19,'rosette connects to crown');
  assert.ok(Math.max(...leaves.map(p=>p[1]))>.9);
  assert.ok(leaves.some(p=>p[0]>.3)&&leaves.some(p=>p[0]<-.3),'leaves spread around crown');
  for(const part of crop.parts){
    assert.ok(part.positions.every(Number.isFinite));
    assert.equal(part.uvs.length,part.positions.length/3*2);
    for(let i=0;i<part.indices.length;i+=3){
      const [a,b,c]=part.indices.slice(i,i+3).map(j=>points(part)[j]);
      const u=b.map((v,k)=>v-a[k]),v=c.map((n,k)=>n-a[k]);
      assert.ok(Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])>1e-10);
    }
  }
});
