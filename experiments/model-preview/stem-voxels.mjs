// Preview-only geometric candidate. Sample the authored stem triangles in 3D,
// never their screen projection. Leaves and complete flower heads stay untouched.
// Native grass cubes are 1/256 world units; flowers use 10/256 per recipe unit
// (flora_vertex.slang / flora::models::WORLD_SCALE). Half a grass edge = .05 here.
export const STEM_CELL_SIZE=.05;
export const STEM_GRID_ORIGIN=[0,-1.2,0];

const sub=(a,b)=>a.map((v,i)=>v-b[i]);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const axes=[[1,0,0],[0,1,0],[0,0,1]];

// Triangle/unit-box separating-axis test, including edge cross-products. An
// AABB-only test would thicken diagonal branches into large rectangular slabs.
function overlaps(triangle,center){
  const points=triangle.map(p=>sub(p,center));
  const edges=points.map((p,i)=>sub(points[(i+1)%3],p));
  const candidates=[...axes,cross(edges[0],edges[1]),...edges.flatMap(edge=>axes.map(axis=>cross(edge,axis)))];
  return candidates.every(axis=>{
    const radius=axis.reduce((sum,v)=>sum+Math.abs(v),0)*(.5+1e-9);
    const projections=points.map(p=>p.reduce((sum,v,i)=>sum+v*axis[i],0));
    return Math.min(...projections)<=radius&&Math.max(...projections)>=-radius;
  });
}

export function voxelizeStem(part){
  const points=Array.from({length:part.positions.length/3},(_,i)=>
    part.positions.slice(i*3,i*3+3).map((v,k)=>(v-STEM_GRID_ORIGIN[k])/STEM_CELL_SIZE));
  const occupied=new Map(),key=p=>p.join(',');
  for(let i=0;i<part.indices.length;i+=3){
    const triangle=part.indices.slice(i,i+3).map(index=>points[index]);
    const low=axes.map((_,k)=>Math.ceil(Math.min(...triangle.map(p=>p[k]))-.5-1e-9));
    const high=axes.map((_,k)=>Math.floor(Math.max(...triangle.map(p=>p[k]))+.5+1e-9));
    for(let x=low[0];x<=high[0];x++)for(let y=low[1];y<=high[1];y++)for(let z=low[2];z<=high[2];z++){
      const cell=[x,y,z],id=key(cell);
      if(!occupied.has(id)&&overlaps(triangle,cell))occupied.set(id,cell);
    }
  }
  // Only exposed cube faces, with independent face vertices for crisp normals.
  // All authored stems are thinner than one cell: conservative surface sampling
  // covers their cross-section; this is not a general thick-solid voxelizer.
  const positions=[],indices=[];
  const cells=[...occupied.values()].sort((a,b)=>a[0]-b[0]||a[1]-b[1]||a[2]-b[2]);
  for(const cell of cells)for(let axis=0;axis<3;axis++)for(const sign of [-1,1]){
    const neighbor=cell.slice();neighbor[axis]+=sign;
    if(occupied.has(key(neighbor)))continue;
    const u=(axis+1)%3,v=(axis+2)%3,first=positions.length/3;
    for(const [du,dv]of [[-.5,-.5],[.5,-.5],[.5,.5],[-.5,.5]]){
      const p=cell.slice();p[axis]+=sign*.5;p[u]+=du;p[v]+=dv;
      positions.push(...p.map((n,k)=>STEM_GRID_ORIGIN[k]+n*STEM_CELL_SIZE));
    }
    indices.push(...(sign>0?[0,1,2,0,2,3]:[0,2,1,0,3,2]).map(n=>first+n));
  }
  return {positions,indices,cells,cellSize:STEM_CELL_SIZE};
}
