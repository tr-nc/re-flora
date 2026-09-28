// Original browser-only strawberry study: rounded white petals, yellow stamens,
// pointed sepals and toothed trifoliate leaves. Same parts/head contract as the
// published flowers; no renderer, camera or pixel-processing policy lives here.
const TAU=2*Math.PI;
const add=(a,b)=>a.map((v,i)=>v+b[i]);
const sub=(a,b)=>a.map((v,i)=>v-b[i]);
const mul=(a,s)=>a.map(v=>v*s);
const mix=(a,b,t)=>add(a,mul(sub(b,a),t));
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const unit=a=>mul(a,1/Math.hypot(...a));
function part(name,material,head=null){return {name,material,head,positions:[],indices:[]};}
function fan(mesh,center,rim){
  const first=mesh.positions.length/3;mesh.positions.push(...center,...rim.flat());
  for(let i=0;i<rim.length;i++)mesh.indices.push(first,first+1+i,first+1+(i+1)%rim.length);
}
function tube(mesh,a,b,radius){
  const axis=unit(sub(b,a)),u=unit(cross(axis,[0,0,1])),v=cross(axis,u);
  const rings=[a,b].map(p=>Array.from({length:5},(_,i)=>add(p,add(mul(u,radius*Math.cos(i*TAU/5)),mul(v,radius*Math.sin(i*TAU/5))))));
  const first=mesh.positions.length/3;mesh.positions.push(...rings.flat(2));
  for(let i=0;i<5;i++){
    const next=(i+1)%5;
    mesh.indices.push(first+i,first+5+i,first+5+next,first+i,first+5+next,first+next);
  }
  fan(mesh,a,rings[0].slice().reverse());fan(mesh,b,rings[1]);
}
function leaf(mesh,start,end,width){
  const axis=sub(end,start),side=unit(cross(axis,[0,0,1]));
  const rim=Array.from({length:24},(_,i)=>{
    const angle=i*TAU/24,t=(1+Math.cos(angle))/2;
    return add(mix(start,end,t),mul(side,Math.sin(angle)*width*(i%2?.78:1)));
  });
  fan(mesh,add(mix(start,end,.48),[0,0,.055]),rim);
}
function headGeometry(parts,index,anchor,radius,azimuth,p){
  const petals=part(`Flower ${index+1} petals`,'petalColor',index);
  const throat=part(`Flower ${index+1} throat`,'innerColor',index);
  const center=part(`Flower ${index+1} center`,'centerColor',index);
  const calyx=part(`Flower ${index+1} calyx`,'stemColor',index);
  for(let i=0;i<5;i++){
    const angle=i*TAU/5+.12;
    const rotate=([x,y,z])=>[x*Math.cos(angle)-y*Math.sin(angle),x*Math.sin(angle)+y*Math.cos(angle),z];
    const point=(x,y)=>rotate([x,y,.045+p.opening*.12*x*x]);
    // Broad rounded lobes, small basal gap and a shallow cup instead of the
    // more radial/notched geranium silhouette.
    fan(petals,point(.57,0),Array.from({length:12},(_,j)=>{
      const a=j*TAU/12;return point(.57+.43*Math.cos(a),.37*Math.sin(a));
    }));
    fan(throat,rotate([.26,0,.08]),[rotate([.1,-.07,.06]),rotate([.37,-.11,.08]),rotate([.4,0,.1]),rotate([.37,.11,.08]),rotate([.1,.07,.06])]);
    const sepals=[rotate([.12,-.09,-.06]),rotate([.63,0,-.055]),rotate([.12,.09,-.06])];
    fan(calyx,rotate([.2,0,-.075]),sepals);
  }
  // Raised yellow receptacle and a ring of visibly separate golden anthers.
  fan(center,[0,0,.24],Array.from({length:12},(_,i)=>[.21*Math.cos(i*TAU/12),.21*Math.sin(i*TAU/12),.075]));
  for(let i=0;i<12;i++){
    const a=i*TAU/12,x=.275*Math.cos(a),y=.275*Math.sin(a);
    fan(center,[x,y,.19],Array.from({length:5},(_,j)=>[x+.046*Math.cos(j*TAU/5),y+.046*Math.sin(j*TAU/5),.11]));
  }
  const tilt=p.tilt*Math.PI/180,az=azimuth*Math.PI/180;
  for(const mesh of [petals,throat,center,calyx]){
    for(let i=0;i<mesh.positions.length;i+=3){
      const [x,y,z]=mesh.positions.slice(i,i+3),yy=y*Math.cos(tilt)+z*Math.sin(tilt),zz=-y*Math.sin(tilt)+z*Math.cos(tilt);
      mesh.positions.splice(i,3,...add(anchor,mul([x*Math.cos(az)+zz*Math.sin(az),yy,-x*Math.sin(az)+zz*Math.cos(az)],radius*p.flowerSize)));
    }
    parts.push(mesh);
  }
}
export function strawberryGeometry(spec,p){
  const stem=part('Stem and branches','stemColor'),leaves=part('Leaves','leafColor'),parts=[stem,leaves],heads=[];
  const root=[0,-1.2,0],joint=[p.bend,-.45,0];
  tube(stem,root,joint,.018);
  for(const [index,[x,y,z,radius,azimuth]]of spec.heads.entries()){
    const anchor=[x+p.bend,y*p.height,z];tube(stem,joint,anchor,.013);
    headGeometry(parts,index,anchor,radius,azimuth,p);
    heads.push({id:index,anchor,label:`花头 ${index+1}`});
  }
  for(let i=0;i<3;i++){
    const sign=i%2?-1:1,start=mix(root,joint,.22+i*.23),size=p.leafSize;
    const palm=add(start,[sign*.28*size,.16*size,.09]);tube(stem,start,palm,.009);
    // One terminal leaflet plus two lateral leaflets on each petiole.
    for(const angle of [-.85,0,.85]){
      const a=sign*.55+angle,end=add(palm,[Math.sin(a)*.46*size,Math.cos(a)*.46*size,.025]);
      leaf(leaves,palm,end,.16*size);
    }
  }
  return {parts,heads};
}
