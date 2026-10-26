import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog,flowerGeometry} from '../../../assets/models/flower-source.mjs';
import {parametricFlower,normalizeFlowerShape} from '../../../assets/models/parametric-flower.mjs';

// Measure in the head's own frame, independent of its presentation tilt.
function points(part){
  return Array.from({length:part.positions.length/3},(_,i)=>part.positions.slice(i*3,i*3+3));
}
function head(id){return flowerGeometry(id,{tilt:0,flowerSize:1});}

test('geranium reads as a shallow cup; forget-me-not has a raised heart and hanging rim',()=>{
  const cup=head('wild-geranium'),umbrella=head('forget-me-not');
  const cupPetals=points(cup.parts[0]),umbrellaPetals=points(umbrella.parts[0]);
  const outer=ps=>ps.filter(([x,y])=>Math.hypot(x,y)>.35);
  const meanZ=ps=>ps.reduce((sum,p)=>sum+p[2],0)/ps.length;
  assert.ok(meanZ(outer(cupPetals))>.12,'cup petals rise visibly above attachment');
  assert.ok(meanZ(outer(umbrellaPetals))<0,'umbrella rim hangs below attachment');
  const centerTop=Math.max(...points(umbrella.parts[1]).map(p=>p[2]));
  assert.ok(centerTop-Math.max(...umbrellaPetals.map(p=>p[2]))>.06,'heart stands above the petal crown');
});

test('relief controls preserve attachment, UV layout and deterministic finite surfaces',()=>{
  const base=parametricFlower({tilt:0});
  for(const setting of [{petalCup:.3},{tipCurl:-.4},{poseVariation:.15},{petalShoulder:.7},{petalArch:.2}]){
    const shaped=parametricFlower({tilt:0,...setting});
    assert.deepEqual(shaped,parametricFlower({tilt:0,...setting}));
    assert.notDeepEqual(shaped.parts[0].positions,base.parts[0].positions);
    assert.deepEqual(shaped.heads,base.heads);
    assert.deepEqual(shaped.socketNormal,base.socketNormal);
    shaped.parts.forEach((part,i)=>{
      assert.ok(part.positions.every(Number.isFinite));
      assert.deepEqual(part.uvs,base.parts[i].uvs);
      assert.deepEqual(part.indices,base.parts[i].indices);
    });
    assert.deepEqual(shaped.parts.slice(1),base.parts.slice(1),'center and calyx are independent');
  }
  const normalized=normalizeFlowerShape({petalCup:NaN,tipCurl:Infinity,poseVariation:-1,petalShoulder:NaN,petalArch:Infinity});
  assert.equal(normalized.petalCup,0);
  assert.equal(normalized.tipCurl,0);
  assert.equal(normalized.poseVariation,0);
  assert.equal(normalized.petalShoulder,.5);
  assert.equal(normalized.petalArch,0);
});

test('transverse cupping follows upright, horizontal and hanging petal surfaces',()=>{
  for(const opening of [-1,0,1]){
    const flat=parametricFlower({opening,tilt:0,petalCup:0});
    const cupped=parametricFlower({opening,tilt:0,petalCup:.25});
    const before=points(flat.parts[0]),after=points(cupped.parts[0]);
    const uv=flat.parts[0].uvs;
    let checked=0;
    before.forEach((p,i)=>{
      // UVs identify mid-petal edges through the public mesh, not row indices.
      if(Math.abs(uv[i*2]-.025)>1e-8||Math.abs(uv[i*2+1]-.5)>1e-8)return;
      const q=after[i],radialChange=Math.hypot(q[0],q[1])-Math.hypot(p[0],p[1]);
      assert.ok(q[2]>p[2],'cup edge rises along the local surface normal');
      if(opening>0)assert.ok(radialChange<-.05,'upright petals wrap inward');
      else if(opening<0)assert.ok(radialChange>.05,'hanging petals fold outward');
      else assert.ok(Math.abs(radialChange)<1e-8,'horizontal cup keeps its radius');
      checked++;
    });
    assert.equal(checked,5);
    assert.deepEqual(cupped.parts.slice(1),flat.parts.slice(1));
  }
});

test('all six species carry multi-texel depth and keep their complete head/socket contract',()=>{
  for(const spec of flowerCatalog.filter(s=>s.kind!=='root_crop')){
    const model=head(spec.id),petals=points(model.parts[0]);
    const depth=petals.map(p=>p[2]);
    const diameter=2*Math.max(...petals.map(p=>Math.hypot(p[0],p[1])));
    assert.ok((Math.max(...depth)-Math.min(...depth))/diameter>.12,`${spec.id}: meaningful side relief, not a thin disk`);
    assert.deepEqual(model.heads,[{id:0,anchor:[0,0,0],label:'完整花头'}]);
    assert.deepEqual(model.socketNormal,[0,0,1]);
  }
  const tulip=points(head('tulip').parts[0]);
  assert.ok(Math.max(...tulip.map(p=>p[2]))>.4,'tulip retains upright cup walls');
  const cone=head('coneflower');
  assert.ok(Math.max(...points(cone.parts[1]).map(p=>p[2]))>.28,'cone remains tall');
  assert.ok(Math.min(...points(cone.parts[0]).map(p=>p[2]))<-.35,'rays hang below the cone');
});
