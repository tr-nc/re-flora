// Geometry-constrained eight-neighbour repair, independent of models/rendering.
// A coverage object belongs to ONE repair group, never all scene objects at once.
const EPSILON = .0001;

export function projectedCoverage(triangles, size) {
  const words=Math.max(1,Math.ceil(triangles.length/32));
  const bits=new Uint32Array(size*size*words);
  const depth=new Float64Array(size*size).fill(Infinity);
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
          const pixel=y*size+x;
          bits[pixel*words+(index>>>5)] |= 1 << (index&31);
          const weights=nearestWeights(triangle,x+.5,y+.5);
          depth[pixel]=Math.min(depth[pixel],weights.reduce((sum,w,j)=>sum+w*(triangle[j][2]??0),0));
        }
      }
    }
  });
  return {
    depth,
    has(pixel) { for(let w=0;w<words;w++) if(bits[pixel*words+w]) return true; return false; },
    shares(a,b) { for(let w=0;w<words;w++) if(bits[a*words+w]&bits[b*words+w]) return true; return false; },
  };
}

function nearestWeights([a,b,c],x,y) {
  const cross=(p,q)=>(p[0]-x)*(q[1]-y)-(p[1]-y)*(q[0]-x);
  const area=cross(a,b)+cross(b,c)+cross(c,a);
  if(Math.abs(area)>1e-10) {
    const w=[cross(b,c)/area,cross(c,a)/area,cross(a,b)/area];
    if(w.every(v=>v>=0)) return w;
  }
  let best=Infinity,result;
  const vertices=[a,b,c];
  for(let i=0;i<3;i++) {
    const j=(i+1)%3,p=vertices[i],q=vertices[j],dx=q[0]-p[0],dy=q[1]-p[1];
    const t=Math.max(0,Math.min(1,((x-p[0])*dx+(y-p[1])*dy)/Math.max(dx*dx+dy*dy,1e-20)));
    const distance=(p[0]+dx*t-x)**2+(p[1]+dy*t-y)**2;
    if(distance<best) { best=distance;result=[0,0,0];result[i]=1-t;result[j]=t; }
  }
  return result;
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
  const mask=Uint8Array.from(original,Boolean), additions=new Uint8Array(mask.length), bridges=[];
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
        if(!support.shares(pixel,next)) return;
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
    const halves=bridge.map(pixel=>{
      const path=[];
      while(pixel!==-1) { path.push(pixel);pixel=previous[pixel]; }
      return path;
    });
    const path=[...halves[0].reverse(),...halves[1]];
    bridges.push(path);
    for(const pixel of path) if(!mask[pixel]) { mask[pixel]=1;additions[pixel]=255; }
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
  return {additions, bridges, before, after:components.count, added:additions.reduce((sum,v)=>sum+Boolean(v),0)};
}
