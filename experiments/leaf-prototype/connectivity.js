// THROWAWAY: geometry-constrained, eight-neighbour repair of a tiny pixel tile.
// Keep all original pixels. Add shortest bridges, not the full coverage envelope.
const EPSILON = .0001;

export function projectedCoverage(triangles, size) {
  if (triangles.length > 32) throw new Error('Prototype coverage supports at most 32 source triangles');
  const support = new Uint32Array(size*size);
  triangles.forEach((triangle, index) => {
    const lo = [0,1].map(axis=>Math.min(...triangle.map(p=>p[axis])));
    const hi = [0,1].map(axis=>Math.max(...triangle.map(p=>p[axis])));
    const [a,b,c] = triangle;
    const winding = (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]) >= 0 ? 1 : -1;
    const edges = triangle.map((p,i)=>{
      const q=triangle[(i+1)%3], dx=q[0]-p[0], dy=q[1]-p[1];
      return {p,dx,dy,radius:(.5+EPSILON)*(Math.abs(dx)+Math.abs(dy))};
    });
    for(let y=Math.max(0,Math.ceil(lo[1]-1-EPSILON));y<=Math.min(size-1,Math.floor(hi[1]+EPSILON));y++) {
      for(let x=Math.max(0,Math.ceil(lo[0]-1-EPSILON));x<=Math.min(size-1,Math.floor(hi[0]+EPSILON));x++) {
        if(edges.every(({p,dx,dy,radius})=>winding*(dx*(y+.5-p[1])-dy*(x+.5-p[0]))>=-radius)) {
          support[y*size+x] |= 1 << index;
        }
      }
    }
  });
  return support;
}

function neighbours(pixel, size, visit) {
  const x=pixel%size, y=Math.floor(pixel/size);
  for(let dy=-1;dy<=1;dy++) for(let dx=-1;dx<=1;dx++) {
    if((dx||dy) && x+dx>=0 && x+dx<size && y+dy>=0 && y+dy<size) visit(pixel+dy*size+dx);
  }
}

export function labelComponents(mask, size) {
  const labels = new Int32Array(mask.length), queue = new Int32Array(mask.length);
  let count=0;
  for(let i=0;i<mask.length;i++) if(mask[i] && !labels[i]) {
    labels[i]=++count;
    let head=0, tail=0; queue[tail++]=i;
    while(head<tail) neighbours(queue[head++],size,next=>{
      if(mask[next] && !labels[next]) { labels[next]=count; queue[tail++]=next; }
    });
  }
  return {labels,count};
}

export function repairCoverage(original, support, size) {
  const mask=Uint8Array.from(original,Boolean), additions=new Uint8Array(mask.length);
  let components=labelComponents(mask,size);
  const before=components.count;
  while(components.count>1) {
    // Every existing component is a source. Distances count ONLY missing pixels;
    // diagonal and cardinal steps both cost one. A diagonal connection is done.
    const owner=components.labels.slice(), distance=new Int32Array(mask.length);
    const previous=new Int32Array(mask.length).fill(-1), queue=new Int32Array(mask.length);
    let head=0,tail=0,bestCost=Infinity,bridge=null;
    for(let i=0;i<mask.length;i++) if(mask[i]) queue[tail++]=i;
    while(head<tail) {
      const pixel=queue[head++];
      neighbours(pixel,size,next=>{
        // Adjacent conservative cells alone are not a licence to glue shapes:
        // a projected source triangle must support this step on both sides.
        if(!(support[pixel]&support[next])) return;
        if(!owner[next]) {
          owner[next]=owner[pixel]; distance[next]=distance[pixel]+1;
          previous[next]=pixel; queue[tail++]=next;
        } else if(owner[next]!==owner[pixel]) {
          const cost=distance[pixel]+distance[next];
          if(cost<bestCost) { bestCost=cost; bridge=[pixel,next]; }
        }
      });
    }
    if(!bridge) break; // No geometry-supported bridge: leave separate shapes alone.
    for(let pixel of bridge) while(pixel!==-1) {
      if(!mask[pixel]) { mask[pixel]=1; additions[pixel]=255; }
      pixel=previous[pixel];
    }
    components=labelComponents(mask,size);
  }
  // Shared bridges can make earlier additions redundant. Remove any addition
  // whose deletion preserves eight-connectivity; never delete an original pixel.
  // This is locally minimal, not a claim to solve the global Steiner-tree problem.
  let changed=true;
  while(changed) {
    changed=false;
    for(let i=mask.length-1;i>=0;i--) if(additions[i]) {
      mask[i]=0;
      if(labelComponents(mask,size).count===components.count) { additions[i]=0; changed=true; }
      else mask[i]=1;
    }
  }
  return {additions, before, after:components.count, added:additions.reduce((sum,v)=>sum+Boolean(v),0)};
}
