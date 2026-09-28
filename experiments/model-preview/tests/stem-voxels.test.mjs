import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {flowerCatalog,flowerGeometry} from '../models/flower-catalog.mjs';
import {voxelizeStem,STEM_CELL_SIZE,STEM_GRID_ORIGIN} from '../stem-voxels.mjs';

const stemFor=(spec,settings={})=>flowerGeometry(spec.id,settings).parts.find(p=>p.head===null&&p.material==='stemColor');
test('stem cell edge is half native grass at the published flower world scale',()=>{
  const native=readFileSync(new URL('../../../shader/slang/flora_vertex.slang',import.meta.url),'utf8');
  const flowers=readFileSync(new URL('../../../src/flora/models.rs',import.meta.url),'utf8');
  const grass=native.match(/FLORA_SCALING_FACTOR = ([\d.]+) \/ ([\d.]+)/);
  const scale=flowers.match(/WORLD_SCALE: f32 = ([\d.]+) \/ ([\d.]+)/);
  assert.ok(grass&&scale,'update the explicit preview scale contract if native units change');
  assert.equal(STEM_CELL_SIZE*(+scale[1]/+scale[2]),(+grass[1]/+grass[2])/2);
});
test('all preview stems stay connected on a deterministic 3D lattice',()=>{
  for(const spec of flowerCatalog)for(const settings of [{},{height:1.15,bend:-.25,leafSize:1.35}]){
    const part=stemFor(spec,settings),before=JSON.stringify(part),result=voxelizeStem(part);
    assert.equal(JSON.stringify(part),before,'never mutate the shared source recipe');
    assert.deepEqual(result,voxelizeStem(part));
    assert.ok(result.cells.length>20&&result.cells.length<2000,`${spec.id}: bounded stem cells`);
    const remaining=new Set(result.cells.map(p=>p.join(','))),queue=[result.cells[0]];
    remaining.delete(queue[0].join(','));
    for(let i=0;i<queue.length;i++)for(let x=-1;x<=1;x++)for(let y=-1;y<=1;y++)for(let z=-1;z<=1;z++){
      if(Math.abs(x)+Math.abs(y)+Math.abs(z)!==1)continue;
      const cell=queue[i].map((n,k)=>n+[x,y,z][k]);
      if(remaining.delete(cell.join(',')))queue.push(cell);
    }
    assert.equal(remaining.size,0,`${spec.id}: no detached branch voxels`);
    for(let i=0;i<part.positions.length;i+=3){
      const p=part.positions.slice(i,i+3);
      assert.ok(result.cells.some(cell=>cell.every((n,k)=>Math.abs(STEM_GRID_ORIGIN[k]+n*STEM_CELL_SIZE-p[k])<=STEM_CELL_SIZE/2+1e-8)),`${spec.id}: authored endpoints remain covered`);
    }
    const occupied=new Set(result.cells.map(p=>p.join(',')));
    let faces=0;
    for(const cell of result.cells)for(let axis=0;axis<3;axis++)for(const sign of [-1,1]){
      const next=cell.slice();next[axis]+=sign;
      if(!occupied.has(next.join(',')))faces++;
    }
    assert.equal(result.indices.length,faces*6,'one quad per exposed cube face');
    assert.equal(result.positions.length,faces*12);
    for(const p of result.positions)assert.ok(Number.isFinite(p));
  }
});
test('diagonal triangles do not fill their whole bounding box',()=>{
  const part={positions:[0,-1.2,0,.5,-.7,0,.501,-.7,0],indices:[0,1,2]};
  const {cells}=voxelizeStem(part);
  assert.ok(cells.length<40);
  assert.ok(cells.every(([x,y])=>Math.abs(x-y)<=1));
});
