// Authoring recipe, not a second game mesh. Publish with scripts/publish-leaf-model.mjs.
export const leafDefaults={width:1,widestPoint:.5,fold:.28,curl:.35,season:.25,veins:.5,stemTint:'#FFFFFF',backTint:'#FFFFFF',light:-35,transmission:.45,steps:6};
export function leafGeometry(state=leafDefaults){
  const positions=[],uvs=[],indices=[];
  function add(x,t,u,stem=false){
    const z=state.fold*Math.abs(x)+state.curl*(t*t*t-.15)*.7+.055*Math.sin(t*3.6)*x;
    positions.push(x,(t-.5)*2.25,z);uvs.push(stem?2:u,t);return positions.length/3-1;
  }
  // Tilt the existing smooth taper, then normalize so moving the widest point
  // does not also change the width slider's effective maximum. The end vertices
  // remain pointed; at the extremes the widest sampled row is near an end.
  const profile=t=>Math.pow(Math.sin(Math.PI*t),.82)*(1.08-.24*t);
  const tilt=12*((state.widestPoint??.5)-.5);
  const tapered=t=>profile(t)*Math.exp(tilt*(t-.5));
  const rowSpan=t=>1.+.05*Math.sin(t*12)+.95+.035*Math.cos(t*15);
  const baselineMax=Math.max(...Array.from({length:7},(_,i)=>profile((i+1)/8)*rowSpan((i+1)/8)));
  const tiltedMax=Math.max(...Array.from({length:7},(_,i)=>tapered((i+1)/8)*rowSpan((i+1)/8)));
  const root=add(0,0,.5),rows=[];
  for(let i=1;i<8;i++){
    const t=i/8,halfWidth=.58*state.width*tapered(t)*baselineMax/tiltedMax;
    const offset=.035*Math.sin(t*Math.PI*1.5);
    rows.push([add(offset-halfWidth*(1.+.05*Math.sin(t*12.)),t,0),add(offset,t,.5),add(offset+halfWidth*(.95+.035*Math.cos(t*15.)),t,1)]);
  }
  indices.push(root,rows[0][1],rows[0][0],root,rows[0][2],rows[0][1]);
  for(let i=0;i<rows.length-1;i++){
    const a=rows[i],b=rows[i+1];
    for(let j=0;j<2;j++)indices.push(a[j],a[j+1],b[j+1],a[j],b[j+1],b[j]);
  }
  const tip=add(-.035,1,.5),end=rows.at(-1);indices.push(end[0],end[1],tip,end[1],end[2],tip);
  const stemRows=[-.15,-.075,.008].map(t=>{
    const x=.09*Math.pow(t/.15,2),w=.014*(1+t*2);return [add(x-w,t,2,true),add(x+w,t,2,true)];
  });
  for(let i=0;i<2;i++){const a=stemRows[i],b=stemRows[i+1];indices.push(a[0],a[1],b[1],a[0],b[1],b[0]);}
  const p=new Float32Array(positions),normals=new Float32Array(p.length);
  // Same indexed area-weighted normals as Three.js computeVertexNormals.
  for(let i=0;i<indices.length;i+=3){
    const [a,b,c]=indices.slice(i,i+3).map(v=>v*3);
    const x=p[c]-p[b],y=p[c+1]-p[b+1],z=p[c+2]-p[b+2],u=p[a]-p[b],v=p[a+1]-p[b+1],w=p[a+2]-p[b+2];
    const n=[y*w-z*v,z*u-x*w,x*v-y*u];
    for(const index of [a,b,c])for(let axis=0;axis<3;axis++)normals[index+axis]+=n[axis];
  }
  for(let i=0;i<normals.length;i+=3){const length=Math.hypot(normals[i],normals[i+1],normals[i+2])||1;for(let axis=0;axis<3;axis++)normals[i+axis]/=length;}
  return {positions:p,normals,uvs:new Float32Array(uvs),indices:new Uint16Array(indices)};
}
