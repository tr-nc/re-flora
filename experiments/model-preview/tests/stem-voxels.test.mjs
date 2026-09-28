import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {flowerCatalog,flowerGeometry} from '../models/flower-catalog.mjs';
import {singleStemFlower} from '../models/single-stem-flower.mjs';
import {stemColumn,voxelStemSurface,STEM_CELL_SIZE} from '../stem-voxels.mjs';
const near=(a,b)=>assert.ok(Math.abs(a-b)<1e-8,`${a} != ${b}`);

test('stem cell edge is half native grass at the published flower world scale',()=>{
  const native=readFileSync(new URL('../../../shader/slang/flora_vertex.slang',import.meta.url),'utf8');
  const flowers=readFileSync(new URL('../../../src/flora/models.rs',import.meta.url),'utf8');
  const grass=native.match(/FLORA_SCALING_FACTOR = ([\d.]+) \/ ([\d.]+)/);
  const scale=flowers.match(/WORLD_SCALE: f32 = ([\d.]+) \/ ([\d.]+)/);
  assert.ok(grass&&scale,'update the explicit preview scale contract if native units change');
  assert.equal(STEM_CELL_SIZE*(+scale[1]/+scale[2]),(+grass[1]/+grass[2])/2);
});
function sectionPoints(mesh,y){
  const points=[];
  for(let i=0;i<mesh.indices.length;i+=3){
    const triangle=mesh.indices.slice(i,i+3).map(index=>mesh.positions.slice(index*3,index*3+3));
    for(let j=0;j<3;j++){
      const a=triangle[j],b=triangle[(j+1)%3];
      if((a[1]<y&&b[1]>y)||(a[1]>y&&b[1]<y)){
        const t=(y-a[1])/(b[1]-a[1]);points.push(a.map((n,k)=>n+(b[k]-n)*t));
      }
    }
  }
  return points;
}

test('every horizontal layer contains exactly one cube, including maximum bends',()=>{
  for(const topY of [.42,.86,1.495])for(const bend of [-.25,0,.25]){
    const column=stemColumn(topY,bend),mesh=voxelStemSurface(column),s=column.cellSize;
    assert.deepEqual(column,stemColumn(topY,bend));
    near(column.tip[1]-column.root[1],column.cells.length*s);
    near(column.tip[0],bend);
    assert.ok(column.tip[1]>=topY&&column.tip[1]<topY+s+1e-8);
    assert.ok(mesh.positions.every(Number.isFinite));
    assert.ok(mesh.indices.every(i=>i>=0&&i<mesh.positions.length/3));
    let expectedCaps=2*s*s;
    for(const [i,cell]of column.cells.entries()){
      assert.equal(cell.layer,i);
      near(cell.center[1],column.root[1]+(i+.5)*s);
      if(i){
        const previous=column.cells[i-1].center,dx=Math.abs(cell.center[0]-previous[0]),dz=Math.abs(cell.center[2]-previous[2]);
        near(cell.center[1]-previous[1],s);assert.ok(dx<s&&dz<s,'shared face area, no gap');
        expectedCaps+=2*(s*s-(s-dx)*(s-dz));
      }
      // Check the actual triangles, not just diagnostic cell metadata. Each
      // interior slice is one square of the fixed grass-relative width.
      for(const offset of [-.31,0,.31]){
        const y=cell.center[1]+offset*s,points=sectionPoints(mesh,y);assert.ok(points.length>0);
        for(const axis of [0,2]){
          near(Math.min(...points.map(p=>p[axis])),cell.center[axis]-s/2);
          near(Math.max(...points.map(p=>p[axis])),cell.center[axis]+s/2);
        }
      }
    }
    let capArea=0;
    for(let i=0;i<mesh.indices.length;i+=3){
      const [a,b,c]=mesh.indices.slice(i,i+3).map(index=>mesh.positions.slice(index*3,index*3+3));
      if(Math.abs(a[1]-b[1])<1e-10&&Math.abs(a[1]-c[1])<1e-10)capArea+=Math.abs((b[0]-a[0])*(c[2]-a[2])-(b[2]-a[2])*(c[0]-a[0]))/2;
    }
    near(capArea,expectedCaps); // no duplicate coincident internal caps
  }
});

test('all flowers use the fixed voxel stalk, one terminal head and directly attached leaves',()=>{
  for(const spec of flowerCatalog)for(const shape of [{},{height:.75,bend:-.25,leafSize:1.35},{height:1.15,bend:.25,leafSize:.6}]){
    const settings={...spec.defaults,...shape},authored=flowerGeometry(spec.id,settings),snapshot=JSON.stringify(authored);
    const b=singleStemFlower(authored,settings,spec.leafRootVertex);
    assert.deepEqual(singleStemFlower(authored,{...settings,voxelStems:false},spec.leafRootVertex),b,'obsolete mode input cannot restore low-poly stems');
    assert.equal(JSON.stringify(authored),snapshot,'native recipes never mutate');
    assert.equal(b.heads.length,1);
    assert.deepEqual(b.heads[0].anchor,b.column.tip);
    const expectedStem=voxelStemSurface(b.column);
    assert.deepEqual(b.parts[0].positions,expectedStem.positions);
    assert.deepEqual(b.parts[0].indices,expectedStem.indices);
    assert.equal(b.parts.filter(part=>part.head===null&&part.material==='stemColor').length,1);
    const sourceHead=authored.heads[0];
    const headParts=b.parts.filter(part=>part.head===0);
    assert.equal(headParts.length,4,'retain the complete terminal flower, including calyx');
    const calyx=headParts.find(part=>part.material==='stemColor');
    assert.ok(calyx&&calyx.name.includes('calyx'),'stem-colored calyx is still owned by the flower object');
    assert.equal(calyx.head,0);
    assert.ok(calyx.indices.length>0);
    for(const part of headParts){
      const original=authored.parts.find(p=>p.head===sourceHead.id&&p.material===part.material);
      assert.deepEqual(part.indices,original.indices);
      part.positions.forEach((n,i)=>near(n-b.column.tip[i%3],original.positions[i]-sourceHead.anchor[i%3]));
    }
    const leaf=b.parts.find(part=>part.material==='leafColor'),originalLeaf=authored.parts.find(part=>part.material==='leafColor');
    assert.deepEqual(leaf.indices,originalLeaf.indices);
    for(const attachment of b.leafAttachments){
      assert.ok(b.column.cells.some(cell=>cell.center.every((n,k)=>n===attachment.root[k])));
      attachment.vertices.forEach(i=>{for(let k=0;k<3;k++)near(leaf.positions[i*3+k]-attachment.root[k],originalLeaf.positions[i*3+k]-attachment.sourceRoot[k]);});
      leaf.positions.slice(attachment.rootIndex*3,attachment.rootIndex*3+3).forEach((n,k)=>near(n,attachment.root[k]));
    }
  }
});

test('invalid/disconnected column requests are rejected instead of adding lateral voxels',()=>{
  for(const top of [-2,NaN,Infinity])assert.throws(()=>stemColumn(top));
  assert.throws(()=>stemColumn(.5,100));
  assert.throws(()=>stemColumn(.5,NaN));
});
