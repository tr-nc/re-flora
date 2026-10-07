import test from 'node:test';
import assert from 'node:assert/strict';
import {animalCatalog,animalGeometry,animalPose} from '../models/animal-geometry.mjs';

test('bee and kingfisher have deterministic opaque finite meshes and explicit wing groups',()=>{
  assert.deepEqual(animalCatalog.map(s=>s.id),['bee','kingfisher']);
  for(const id of ['sparrow','swallow']){assert.throws(()=>animalGeometry(id),/Unknown animal/);assert.throws(()=>animalPose(id,0),/Unknown animal/);}
  const shapes=new Set();
  for(const spec of animalCatalog){
    const geometry=animalGeometry(spec.id);assert.deepEqual(animalGeometry(spec.id),geometry);shapes.add(JSON.stringify(geometry));
    assert.deepEqual(Object.keys(geometry.hinges),['leftWing','rightWing']);
    const groups=new Set();let triangles=0;
    for(const mesh of geometry.meshes){
      groups.add(mesh.group);assert.ok(spec.colors[mesh.color]||['eye','shine','iris','billSeam'].includes(mesh.color));
      assert.ok(mesh.positions.every(Number.isFinite));assert.equal(mesh.positions.length%3,0);
      if(mesh.appearance){
        assert.ok(['head','body','wing','tail','bill'].includes(mesh.appearance));
        assert.equal(mesh.uvs.length,mesh.positions.length/3*2);assert.ok(mesh.uvs.every(Number.isFinite));
      }
      if(mesh.normals){
        assert.equal(mesh.normals.length,mesh.positions.length);assert.ok(mesh.normals.every(Number.isFinite));
        for(let i=0;i<mesh.normals.length;i+=3)assert.ok(Math.abs(Math.hypot(...mesh.normals.slice(i,i+3))-1)<1e-12);
      }
      assert.ok(mesh.indices.length>0);assert.equal(mesh.indices.length%3,0);
      assert.ok(mesh.indices.every(i=>Number.isInteger(i)&&i>=0&&i<mesh.positions.length/3));
      for(let i=0;i<mesh.indices.length;i+=3){
        const [a,b,c]=mesh.indices.slice(i,i+3).map(j=>mesh.positions.slice(j*3,j*3+3)),u=b.map((v,j)=>v-a[j]),v=c.map((v,j)=>v-a[j]);
        assert.ok(Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])>1e-10);
      }triangles+=mesh.indices.length/3;
    }
    assert.deepEqual([...groups].sort(),['body','leftWing','rightWing']);assert.ok(triangles>200&&triangles<(spec.id==='bee'?4000:10000));
  }
  assert.equal(shapes.size,2);
  const bee=animalGeometry('bee').meshes;
  assert.equal(bee.filter(m=>m.name.startsWith('Leg ')).length,6);
  assert.equal(bee.filter(m=>/^(Forewing|Hindwing)/.test(m.name)).length,4);
  assert.equal(bee.filter(m=>m.name.startsWith('Abdomen stripe')).length,7);
});
test('kingfisher has a long flattened bill, short tail, plumage regions and two forward toes plus a hind toe',()=>{
  const {meshes}=animalGeometry('kingfisher'),bill=meshes.find(m=>m.name==='Kingfisher long bill');
  const axis=(m,index)=>m.positions.filter((_,i)=>i%3===index),extent=(m,index)=>Math.max(...axis(m,index))-Math.min(...axis(m,index));
  assert.ok(extent(bill,2)>3*extent(bill,0));assert.ok(extent(bill,0)>extent(bill,1));
  const head=meshes.filter(m=>m.name.startsWith('Kingfisher head'));
  assert.ok(Math.max(...axis(bill,2))-Math.max(...head.flatMap(m=>axis(m,2)))>.75);
  assert.deepEqual([...new Set(meshes.filter(m=>m.appearance).map(m=>m.appearance))].sort(),['bill','body','head','tail','wing']);
  for(const side of [-1,1]){
    const toes=meshes.filter(m=>m.name.startsWith(`Toe ${side}:`));assert.equal(toes.length,3);
    assert.equal(toes.filter(m=>m.name.includes('front')).length,2);
    assert.ok(Math.max(...axis(toes.find(m=>m.name.endsWith('rear')),2))<.10);
    const group=side<0?'leftWing':'rightWing';
    assert.equal(meshes.filter(m=>m.group===group&&m.appearance==='wing').length,6);
    assert.equal(meshes.filter(m=>m.group===group&&m.name.startsWith('Primary feather')).length,5);
  }
  const tails=meshes.filter(m=>m.name.startsWith('Short tail'));assert.equal(tails.length,3);
  assert.ok(tails.every(m=>extent(m,2)<.5));
});
test('curved kingfisher feathers have closed subdivided rims without T-junctions',()=>{
  const feathers=animalGeometry('kingfisher').meshes.filter(m=>/^(Folded wing|Primary feather|Primary blue edge|Blue covert|Short tail|Rufous tail tip)/.test(m.name));
  assert.equal(feathers.length,15);
  for(const mesh of feathers){
    const edges=new Map();
    for(let i=0;i<mesh.indices.length;i+=3)for(let j=0;j<3;j++){
      const a=mesh.indices[i+j],b=mesh.indices[i+(j+1)%3],key=[Math.min(a,b),Math.max(a,b)].join(':');
      edges.set(key,(edges.get(key)??0)+1);
    }
    assert.ok([...edges.values()].every(count=>count===2),`open feather rim: ${mesh.name}`);
  }
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
