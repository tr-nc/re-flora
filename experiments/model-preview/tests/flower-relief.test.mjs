import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerGeometry} from '../../../assets/models/flower-source.mjs';
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
  for(const setting of [{petalCup:.3},{tipCurl:-.4},{poseVariation:.15}]){
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
  const normalized=normalizeFlowerShape({petalCup:NaN,tipCurl:Infinity,poseVariation:-1});
  assert.equal(normalized.petalCup,0);
  assert.equal(normalized.tipCurl,0);
  assert.equal(normalized.poseVariation,0);
});
