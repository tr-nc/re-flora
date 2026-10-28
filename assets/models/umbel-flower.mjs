// Original spherical umbel topology, in Y-up attachment-local coordinates.
// A single head made of radially facing six-tepal florets and connected pedicels.
// The native renderer owns the long scape, growth, wind and placement.
const TAU=Math.PI*2;
const add=(a,b)=>a.map((v,i)=>v+b[i]);
const scale=(a,s)=>a.map(v=>v*s);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const unit=a=>scale(a,1/Math.hypot(...a));
export function sphericalUmbel(){
  const parts=[];
  const mesh=name=>{const p={name,material:'palette',head:0,positions:[],uvs:[],indices:[]};parts.push(p);return p;};
  const vertex=(p,v,uv)=>{p.positions.push(...v);p.uvs.push(...uv);return p.positions.length/3-1;};
  const triangle=(p,a,b,c)=>p.indices.push(a,b,c);
  const florets=mesh('Star florets'),stalks=mesh('Radial pedicels'),socket=mesh('Umbel receptacle');
  const center=[0,.72,0],radius=.62,count=120;
  // Fibonacci directions cover the whole sphere, including its underside.
  // Fixed tessellation/seed means asset publication is deterministic.
  for(let i=0;i<count;i++){
    const y=1-2*(i+.5)/count,a=i*2.399963229728653;
    const r=Math.sqrt(1-y*y),normal=[r*Math.cos(a),y,r*Math.sin(a)];
    const u=unit(cross(Math.abs(y)>.9?[1,0,0]:[0,1,0],normal)),v=cross(normal,u);
    const tip=add(center,scale(normal,radius));
    const heart=vertex(florets,tip,[.87,.25]);
    for(let petal=0;petal<6;petal++){
      const angle=petal*TAU/6+i*.37;
      const direction=add(scale(u,Math.cos(angle)),scale(v,Math.sin(angle)));
      const side=add(scale(u,-Math.sin(angle)),scale(v,Math.cos(angle)));
      const shoulder=add(add(tip,scale(direction,.052)),scale(normal,.014));
      const left=vertex(florets,add(shoulder,scale(side,.027)),[.32,.36]);
      const point=vertex(florets,add(add(tip,scale(direction,.105)),scale(normal,.025)),[.32,.93]);
      const right=vertex(florets,add(shoulder,scale(side,-.027)),[.32,.36]);
      triangle(florets,heart,right,point);triangle(florets,heart,point,left);
    }
    // Triangular tapered pedicels are solid in every view, not camera-facing cards.
    const rows=[.06,radius].map((distance,j)=>Array.from({length:3},(_,k)=>{
      const angle=k*TAU/3,width=j===0?.014:.009;
      const offset=add(scale(u,Math.cos(angle)*width),scale(v,Math.sin(angle)*width));
      return vertex(stalks,add(add(center,scale(normal,distance)),offset),[.87,.75]);
    }));
    for(let k=0;k<3;k++){
      const next=(k+1)%3;
      triangle(stalks,rows[0][k],rows[0][next],rows[1][next]);
      triangle(stalks,rows[0][k],rows[1][next],rows[1][k]);
    }
  }
  // Closed green receptacle joins the analytic stem at y=0 to the pedicel hub.
  const rings=[[0,.055],[.62,.09],[.78,.055]].map(([y,r])=>Array.from({length:8},(_,i)=>{
    const a=i*TAU/8;return vertex(socket,[Math.cos(a)*r,y,Math.sin(a)*r],[.87,.75]);
  }));
  for(let j=0;j<rings.length-1;j++)for(let i=0;i<8;i++){
    const k=(i+1)%8;
    triangle(socket,rings[j][i],rings[j+1][i],rings[j+1][k]);
    triangle(socket,rings[j][i],rings[j+1][k],rings[j][k]);
  }
  const bottom=vertex(socket,[0,0,0],[.87,.75]),top=vertex(socket,[0,.78,0],[.87,.75]);
  for(let i=0;i<8;i++){
    const k=(i+1)%8;triangle(socket,bottom,rings[0][i],rings[0][k]);triangle(socket,top,rings[2][k],rings[2][i]);
  }
  return {parts,heads:[{id:0,anchor:[0,0,0],label:'Spherical umbel'}],socketNormal:[0,1,0]};
}
