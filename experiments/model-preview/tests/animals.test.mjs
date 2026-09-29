import test from 'node:test';
import assert from 'node:assert/strict';
import {animalCatalog,animalGeometry,animalPose} from '../models/animal-geometry.mjs';

test('bee and two distinct birds have deterministic opaque finite meshes and explicit wing groups',()=>{
  assert.deepEqual(animalCatalog.map(s=>s.id),['bee','sparrow','swallow']);
  const shapes=new Set();
  for(const spec of animalCatalog){
    const geometry=animalGeometry(spec.id);assert.deepEqual(animalGeometry(spec.id),geometry);shapes.add(JSON.stringify(geometry));
    assert.deepEqual(Object.keys(geometry.hinges),['leftWing','rightWing']);
    const groups=new Set();let triangles=0;
    for(const mesh of geometry.meshes){
      groups.add(mesh.group);assert.ok(spec.colors[mesh.color]||['eye','shine'].includes(mesh.color));
      assert.ok(mesh.positions.every(Number.isFinite));assert.equal(mesh.positions.length%3,0);
      assert.ok(mesh.indices.length>0);assert.equal(mesh.indices.length%3,0);
      assert.ok(mesh.indices.every(i=>Number.isInteger(i)&&i>=0&&i<mesh.positions.length/3));
      for(let i=0;i<mesh.indices.length;i+=3){
        const [a,b,c]=mesh.indices.slice(i,i+3).map(j=>mesh.positions.slice(j*3,j*3+3)),u=b.map((v,j)=>v-a[j]),v=c.map((v,j)=>v-a[j]);
        assert.ok(Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])>1e-10);
      }triangles+=mesh.indices.length/3;
    }
    assert.deepEqual([...groups].sort(),['body','leftWing','rightWing']);assert.ok(triangles>200&&triangles<4000);
  }
  assert.equal(shapes.size,3);
  const bee=animalGeometry('bee').meshes;
  assert.equal(bee.filter(m=>m.name.startsWith('Leg ')).length,6);
  assert.equal(bee.filter(m=>/^(Forewing|Hindwing)/.test(m.name)).length,4);
  assert.equal(bee.filter(m=>m.name.startsWith('Abdomen stripe')).length,7);
  assert.equal(animalGeometry('swallow').meshes.filter(m=>m.name.startsWith('Fork tail')).length,2);
  assert.equal(animalGeometry('sparrow').meshes.filter(m=>m.name.startsWith('Fan tail')).length,3);
});
test('pose sampling is absolute, periodic, mirrored, and distinct between rest and flight',()=>{
  for(const {id}of animalCatalog){
    const duration=id==='bee'?1:2;
    for(const t of [0,.013,.11,.3,.78]){
      const pose=animalPose(id,t,1);assert.ok(Object.values(pose).every(Number.isFinite));
      assert.equal(pose.leftWing,-pose.rightWing);
      const repeated=animalPose(id,t+duration,1);
      for(const key of Object.keys(pose))assert.ok(Math.abs(pose[key]-repeated[key])<1e-12);
      assert.deepEqual(animalPose(id,t,0,.5),animalPose(id,0,0,1.3));
    }
    assert.notDeepEqual(animalPose(id,.013,1),animalPose(id,.11,1));
    assert.deepEqual(animalPose(id,NaN,1),animalPose(id,0,1));
  }
});
