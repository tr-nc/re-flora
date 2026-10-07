import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {soilBagClips,soilBagDefaults,soilBagPresets,soilBagGeometry,soilBagPose,soilBagFlowGeometry} from '../models/soil-bag-geometry.mjs';
import {sampleTime,advanceTime,stepTime} from '../timeline.mjs';

test('browser-only bag adapters and shared controls parse without requiring a GPU or import map',()=>{
  for(const file of ['../viewer.js','../models/index.js','../models/soil-bag.js','../models/soil-bag-stickers.js']){
    execFileSync(process.execPath,['--check',fileURLToPath(new URL(file,import.meta.url))],{stdio:'pipe'});
  }
});

const triangleArea=(positions,a,b,c)=>{
  const points=[a,b,c].map(i=>positions.slice(i*3,i*3+3)),u=points[1].map((v,i)=>v-points[0][i]),v=points[2].map((n,i)=>n-points[0][i]);
  return Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]);
};
const vertices=part=>Array.from({length:part.positions.length/3},(_,i)=>part.positions.slice(i*3,i*3+3));
const rotate=(p,angle)=>[p[0]*Math.cos(angle)-p[1]*Math.sin(angle),p[0]*Math.sin(angle)+p[1]*Math.cos(angle),p[2]];

test('pouch recipe is deterministic and every opaque surface has finite nondegenerate indexed triangles',()=>{
  const base=soilBagGeometry();assert.deepEqual(soilBagGeometry(),base);
  assert.deepEqual(base.parts.map(p=>p.group),['bag','bag','tab','bag','bag','bag']);
  for(const opening of [0,.1,.5,1])for(const fullness of [.35,1,1.25])for(const sideFlatness of [0,.5,1]){
    const recipe=soilBagGeometry({opening,fullness,sideFlatness,labelScale:1.08});
    for(let n=0;n<recipe.parts.length;n++){
      const part=recipe.parts[n];assert.ok(part.positions.every(Number.isFinite));assert.ok(part.uvs.every(Number.isFinite));
      assert.equal(part.positions.length%3,0);assert.equal(part.uvs.length,part.positions.length/3*2);
      assert.deepEqual(part.indices,base.parts[n].indices,'pose edits must update existing buffers, not change topology');
      for(let i=0;i<part.indices.length;i+=3){
        const indices=part.indices.slice(i,i+3);assert.ok(indices.every(j=>Number.isInteger(j)&&j>=0&&j<part.positions.length/3));
        assert.ok(triangleArea(part.positions,...indices)>1e-10,part.name);
      }
    }
  }
});

test('opening separates one right-hand mouth while the rest of the seal stays closed',()=>{
  const closed=soilBagGeometry(),opened=soilBagGeometry({opening:1});
  const a=vertices(closed.parts[0]),b=vertices(opened.parts[0]);let moved=0;
  for(let i=0;i<a.length;i++){
    if(a[i][0]<=.12+1e-8)assert.deepEqual(a[i],b[i],'left body must not gain another opening');
    if(Math.abs(a[i][1]-.82)<1e-8&&Math.abs(b[i][2]-a[i][2])>.001)moved++;
  }
  assert.ok(moved>3);assert.ok(vertices(opened.parts[1]).every(p=>p[0]<=.12+1e-8),'fixed seal cannot cover the right aperture');
  assert.equal(soilBagPose(0,0).opening,0);assert.equal(soilBagPose(soilBagClips[0].duration,0).opening,1);
});

test('side flatness fills upper and lower shoulders without changing peak thickness, rims or topology',()=>{
  const depthAt=(part,y)=>Math.max(...vertices(part).filter(p=>Math.abs(p[1]-y)<1e-8).map(p=>Math.abs(p[2])));
  for(const fullness of [.35,1,1.25])for(const opening of [0,.5,1]){
    const rounded=soilBagGeometry({fullness,opening,sideFlatness:0}),half=soilBagGeometry({fullness,opening,sideFlatness:.5}),flat=soilBagGeometry({fullness,opening,sideFlatness:1});
    for(const y of [-.615,-.41,.41,.615]){
      assert.ok(depthAt(flat.parts[0],y)>depthAt(rounded.parts[0],y)+.015,'flattening must fill both shoulders, not simply squash the whole bag');
      assert.ok(depthAt(half.parts[0],y)>depthAt(rounded.parts[0],y)&&depthAt(half.parts[0],y)<depthAt(flat.parts[0],y));
    }
    for(const y of [-.82,0,.82])assert.equal(depthAt(flat.parts[0],y),depthAt(rounded.parts[0],y),'peak thickness and both sealed rims stay fixed');
    assert.deepEqual(flat.parts.slice(1,5),rounded.parts.slice(1,5),'single opening, lining and tear strip must remain compatible');
    assert.deepEqual(flat.tabHinge,rounded.tabHinge);assert.deepEqual(flat.mouthLocal,rounded.mouthLocal);
    for(let i=0;i<flat.parts.length;i++){
      assert.deepEqual(flat.parts[i].indices,rounded.parts[i].indices);assert.deepEqual(flat.parts[i].uvs,rounded.parts[i].uvs);
      for(let n=0;n<flat.parts[i].positions.length;n++){
        if(n%3!==2)assert.equal(flat.parts[i].positions[n],rounded.parts[i].positions[n]);
        assert.ok(Math.abs(half.parts[i].positions[n]-(flat.parts[i].positions[n]+rounded.parts[i].positions[n])/2)<1e-12,'each vertex interpolates continuously, even when the outermost vertex changes');
      }
    }
    const sticker=flat.parts.at(-1),oldSticker=rounded.parts.at(-1);
    assert.ok(sticker.positions.some((v,i)=>i%3===2&&v>oldSticker.positions[i]+.015),'sticker must follow the changed bag surface');
  }
});

test('side flatness defaults to the approved round profile and invalid or out-of-range values are bounded',()=>{
  assert.equal(soilBagDefaults.sideFlatness,0);
  const original=soilBagGeometry();assert.deepEqual(soilBagGeometry({sideFlatness:0}),original);
  for(const value of [-1,NaN,Infinity,-Infinity])assert.deepEqual(soilBagGeometry({sideFlatness:value}),original);
  assert.deepEqual(soilBagGeometry({sideFlatness:2}),soilBagGeometry({sideFlatness:1}));
  for(const preset of soilBagPresets)assert.equal(preset.values.sideFlatness,0);
});

test('actions are absolute and hold their end pose; pouring starts open and only flows with the mouth down',()=>{
  assert.ok(soilBagClips.every(clip=>clip.loop===false));
  for(const clip of [0,1]){
    const duration=soilBagClips[clip].duration;
    assert.deepEqual(soilBagPose(NaN,clip),soilBagPose(0,clip));
    assert.deepEqual(soilBagPose(duration+10,clip),soilBagPose(duration,clip));
    for(const time of [0,duration*.25,duration*.5,duration*.9,duration]){
      const pose=soilBagPose(time,clip);soilBagPose(0,clip);assert.deepEqual(soilBagPose(time,clip),pose);
      assert.ok([...pose.root,...pose.mouthWorld,...pose.mouthNormal,pose.angle,pose.flow].every(Number.isFinite));
      if(pose.flow>0){assert.equal(pose.opening,1);assert.ok(pose.mouthNormal[1]<-.12);}
      if(clip===0)assert.equal(pose.flow,0);
    }
  }
  const start=soilBagPose(0,1),pour=soilBagPose(1.9,1),end=soilBagPose(3.8,1);
  assert.equal(start.opening,1);assert.equal(start.flow,0);assert.ok(pour.mouthNormal[1]<-.7);assert.ok(pour.flow>.5);
  assert.equal(end.opening,1);assert.equal(end.flow,0);assert.equal(end.angle,0);assert.ok(end.fill<start.fill);
});

test('coarse soil flow starts at the actual mouth and falls in world gravity, not local bag Y',()=>{
  const pose=soilBagPose(1.9,1),flow=soilBagFlowGeometry(pose,1.9),points=vertices(flow),centers=[];
  assert.ok(flow.positions.every(Number.isFinite));
  for(let ring=0;ring<=8;ring++)centers.push(points.slice(ring*7,(ring+1)*7).reduce((sum,p)=>sum.map((v,i)=>v+p[i]/7),[0,0,0]));
  for(let i=0;i<3;i++)assert.ok(Math.abs(centers[0][i]-pose.mouthWorld[i])<1e-12);
  for(let i=1;i<centers.length;i++)assert.ok(centers[i][1]<centers[i-1][1]);
  assert.ok(centers.at(-1)[1]<pose.mouthWorld[1]-.7);
});

test('fixed shared camera framing contains both actions including the opened strip and gravity flow',()=>{
  for(const clip of [0,1])for(const sideFlatness of [0,1])for(let frame=0;frame<=20;frame++){
    const time=soilBagClips[clip].duration*frame/20,pose=soilBagPose(time,clip),recipe=soilBagGeometry({opening:pose.opening,fullness:1.25,sideFlatness,labelScale:1.08,fill:pose.fill});
    const world=[];
    for(const part of recipe.parts)for(let p of vertices(part)){
      if(part.group==='tab')p=rotate(p,pose.tabAngle).map((v,i)=>v+recipe.tabHinge[i]);
      world.push(rotate(p,pose.angle).map((v,i)=>v+pose.root[i]));
    }
    if(pose.flow>0)world.push(...vertices(soilBagFlowGeometry(pose,time)));
    for(const p of world)assert.ok(Math.hypot(p[0]-.03,p[1]+.13,p[2])<3.25/2-.03,'no auto-rescaling or animation crop');
  }
});

test('packaging presets are complete editable starting points, not game defaults',()=>{
  assert.equal(soilBagPresets.length,3);assert.equal(new Set(soilBagPresets.map(p=>p.values.stickerDesign)).size,3);
  for(const preset of soilBagPresets){
    assert.deepEqual(Object.keys(preset.values).sort(),Object.keys(soilBagDefaults).sort());
    for(const key of ['bagColor','labelColor','inkColor'])assert.match(preset.values[key],/^#[a-f0-9]{6}$/i);
  }
});

test('shared nonlooping clock retains exact final keys at every FPS and default looping remains unchanged',()=>{
  const duration=1.65;
  for(let fps=2;fps<=60;fps++){
    assert.equal(sampleTime(duration,duration,fps,false),duration);assert.equal(sampleTime(duration+3,duration,fps,false),duration);
    assert.equal(sampleTime(-1,duration,fps,false),0);assert.equal(advanceTime(duration-.01,.1,duration,1,false),duration);
    assert.equal(advanceTime(duration,.1,duration,1,false),duration);
    assert.equal(sampleTime(duration,duration,fps),0);
    assert.equal(stepTime(0,duration,fps,-1,false),0);assert.equal(stepTime(duration,duration,fps,1,false),duration);
    assert.equal(stepTime(duration,duration,fps,-1,false),(Math.ceil(duration*fps)-1)/fps,'previous frame must not skip the last sampled frame');
    assert.equal(stepTime(0,duration,fps,-1),(Math.ceil(duration*fps)-1)/fps);
  }
});
