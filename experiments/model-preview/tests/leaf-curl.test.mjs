import test from 'node:test';
import assert from 'node:assert/strict';
import {leafGeometry,leafDefaults} from '../../../assets/models/leaf-source.mjs';

function centerline(curl,lengthScale=1){
  const {positions}=leafGeometry({...leafDefaults,curl,length:lengthScale});
  // Root, seven row midpoints, tip (three vertices per row).
  const ids=[0,...Array.from({length:7},(_,i)=>2+i*3),22];
  return ids.map(i=>Array.from(positions.slice(i*3,i*3+3)));
}
function distance(a,b){return Math.hypot(...a.map((v,i)=>v-b[i]));}
function length(points){return points.slice(1).reduce((sum,p,i)=>sum+distance(p,points[i]),0);}

test('leaf length slider scales the midrib without changing its curl behavior or stem length',()=>{
  const short=centerline(0,.65),normal=centerline(0),long=centerline(0,1.2);
  assert.ok(Math.abs(length(short)/length(normal)-.65)<.005);
  assert.ok(Math.abs(length(long)/length(normal)-1.2)<.005);
  const bent=centerline(1.2,1.2);
  assert.ok(Math.abs(length(bent)/length(centerline(1.2))-1.2)<.005);
  const stemLength=scale=>{
    const {positions}=leafGeometry({...leafDefaults,length:scale});
    return Math.hypot(...[0,1,2].map(axis=>positions[23*3+axis]-positions[27*3+axis]));
  };
  assert.ok(Math.abs(stemLength(.65)-stemLength(1.2))<1e-5);
});

test('curl preserves the leaf centerline material length while shortening its Y projection',()=>{
  const flat=centerline(0),base=length(flat);
  for(const curl of [-1.8,-1,-0.35,0.35,1,1.8]){
    const curled=centerline(curl);
    assert.ok(Math.abs(length(curled)-base)<0.012,`curl ${curl}: ${length(curled)} vs ${base}`);
    assert.ok(curled.at(-1)[1]-curled[0][1]<flat.at(-1)[1]-flat[0][1]-0.005,`curl ${curl} did not shorten Y`);
  }
});
