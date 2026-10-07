import test from 'node:test';
import assert from 'node:assert/strict';
import {animalGeometry} from '../models/animal-geometry.mjs';

// Broad profile guardrails from the user's Ceyx erithaca photo, not a claim
// that numbers can decide resemblance. Side-by-side rendered review is still
// required. +Z faces the bill; the reference's body falls behind the head.
const coordinates=(parts,axis)=>parts.flatMap(part=>part.positions.filter((_,i)=>i%3===axis));
const extent=(parts,axis)=>{const values=coordinates(parts,axis);return Math.max(...values)-Math.min(...values);};
const projectedCenter=parts=>{
  let area=0,y=0,z=0;
  for(const part of parts)for(let i=0;i<part.indices.length;i+=3){
    const p=part.indices.slice(i,i+3).map(j=>part.positions.slice(j*3,j*3+3));
    const a=Math.abs((p[1][1]-p[0][1])*(p[2][2]-p[0][2])-(p[1][2]-p[0][2])*(p[2][1]-p[0][1]))/2;
    area+=a;y+=a*(p[0][1]+p[1][1]+p[2][1])/3;z+=a*(p[0][2]+p[1][2]+p[2][2])/3;
  }
  return {y:y/area,z:z/area};
};

test('reference profile has a compact head, not an oversized round helmet',()=>{
  const {meshes}=animalGeometry('kingfisher');
  const head=meshes.filter(part=>part.name.startsWith('Kingfisher head'));
  const body=meshes.filter(part=>part.name.startsWith('Kingfisher body'));
  const ratio=extent(head,1)/extent(body,1);
  assert.ok(ratio>=.38&&ratio<=.56,`head/body height ${ratio.toFixed(3)}; photo approximately .5`);
});

test('reference profile puts the oval body behind the head, rather than standing vertically under it',()=>{
  const {meshes}=animalGeometry('kingfisher');
  // Side-profile eye -> body mass is the photographed diagonal. Measuring
  // just the breast edge is wrong: this bird has a full, nearly vertical
  // upper breast even though its overall body sits strongly behind the eye.
  // Area weighting also avoids a tessellation-density-biased vertex average.
  const body=projectedCenter(meshes.filter(part=>part.name.startsWith('Kingfisher body')));
  const eyes=meshes.filter(part=>part.name.startsWith('Bird eye'));
  const mean=axis=>{const p=coordinates(eyes,axis);return p.reduce((a,b)=>a+b,0)/p.length;};
  const slope=(mean(2)-body.z)/(mean(1)-body.y);
  assert.ok(slope>=.70&&slope<=1.05,`eye-to-body diagonal ${slope.toFixed(3)}; photo approximately .74`);
});
