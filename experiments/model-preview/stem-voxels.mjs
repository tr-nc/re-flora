// Preview-only single-column stems. Native grass edge = 1/256 world units;
// flower recipe scale = 10/256. Half a grass edge is .05 recipe units.
export const STEM_CELL_SIZE=.05;
export const STEM_GRID_ORIGIN=[0,-1.2,0];

export function stemColumn(topY,bend=0){
  if(!Number.isFinite(topY)||topY<=STEM_GRID_ORIGIN[1]||!Number.isFinite(bend))throw new Error('Invalid single-stem extent');
  const count=Math.ceil((topY-STEM_GRID_ORIGIN[1])/STEM_CELL_SIZE);
  const cells=Array.from({length:count},(_,layer)=>{
    const t=count===1?0:layer/(count-1),x=bend*t*t*(3-2*t);
    return {layer,center:[x,STEM_GRID_ORIGIN[1]+(layer+.5)*STEM_CELL_SIZE,0]};
  });
  // Translation only: rotating cubes would put two cells in the same horizontal
  // cross-section. Neighbor footprints must overlap with positive area.
  for(let i=1;i<cells.length;i++)for(const axis of [0,2]){
    if(Math.abs(cells[i].center[axis]-cells[i-1].center[axis])>=STEM_CELL_SIZE)throw new Error('Stem bend disconnects adjacent layers');
  }
  return {cells,cellSize:STEM_CELL_SIZE,root:[...STEM_GRID_ORIGIN],tip:[cells.at(-1).center[0],STEM_GRID_ORIGIN[1]+count*STEM_CELL_SIZE,0]};
}
function surface(){return {positions:[],indices:[]};}
function quad(mesh,points){
  const first=mesh.positions.length/3;mesh.positions.push(...points.flat());
  mesh.indices.push(first,first+1,first+2,first,first+2,first+3);
}
function face(mesh,axis,sign,coordinate,rect){
  const [loU,loV,hiU,hiV]=rect;if(hiU-loU<1e-12||hiV-loV<1e-12)return;
  const u=(axis+1)%3,v=(axis+2)%3;
  const points=[[loU,loV],[hiU,loV],[hiU,hiV],[loU,hiV]].map(([a,b])=>{
    const p=[0,0,0];p[axis]=coordinate;p[u]=a;p[v]=b;return p;
  });
  quad(mesh,sign>0?points:points.reverse());
}
// Rectangle A minus its overlap with B; disjoint exposed strips. This removes
// coincident internal caps without hiding the small ledges between shifted cubes.
function subtract(a,b){
  if(!b)return [a];
  const [x0,z0,x1,z1]=a,ix0=Math.max(x0,b[0]),iz0=Math.max(z0,b[1]),ix1=Math.min(x1,b[2]),iz1=Math.min(z1,b[3]);
  if(ix0>=ix1||iz0>=iz1)return [a];
  return [[x0,z0,ix0,z1],[ix1,z0,x1,z1],[ix0,z0,ix1,iz0],[ix0,iz1,ix1,z1]].filter(([a,b,c,d])=>c>a&&d>b);
}
export function voxelStemSurface(column){
  const mesh=surface(),half=column.cellSize/2;
  const footprint=cell=>cell?[cell.center[2]-half,cell.center[0]-half,cell.center[2]+half,cell.center[0]+half]:null;
  for(const [i,cell]of column.cells.entries()){
    const c=cell.center;
    // The only four vertical faces at this height belong to this one cube.
    for(const axis of [0,2])for(const sign of [-1,1]){
      const u=(axis+1)%3,v=(axis+2)%3;
      face(mesh,axis,sign,c[axis]+sign*half,[c[u]-half,c[v]-half,c[u]+half,c[v]+half]);
    }
    for(const sign of [-1,1])for(const strip of subtract(footprint(cell),footprint(column.cells[i+sign]))){
      face(mesh,1,sign,c[1]+sign*half,strip);
    }
  }
  return mesh;
}
