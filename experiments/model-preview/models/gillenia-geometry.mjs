// Browser-only Gillenia trifoliata study, guided by the supplied reference:
// airy branching reddish stems, separated narrow white petals, small centers,
// and lanceolate toothed leaflets in threes. No strawberry geometry remains.
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
function leaflet(mesh,start,end,width){
  const axis=sub(end,start),side=unit(cross(axis,[0,0,1]));
  // A pointed, long blade with a serrated edge, not a rounded strawberry leaf.
  const edge=sign=>Array.from({length:13},(_,i)=>{
    const t=i/12,serration=i%2?.76:1;
    return add(mix(start,end,t),mul(side,sign*Math.sin(Math.PI*t)*width*serration));
  });
  fan(mesh,add(mix(start,end,.48),[0,0,.025]),[...edge(1),...edge(-1).slice(1,-1).reverse()]);
}
function headGeometry(parts,index,anchor,radius,azimuth,p){
  const petals=part(`Flower ${index+1} petals`,'petalColor',index);
  const throat=part(`Flower ${index+1} throat`,'innerColor',index);
  const center=part(`Flower ${index+1} center`,'centerColor',index);
  const calyx=part(`Flower ${index+1} calyx`,'stemColor',index);
  for(let i=0;i<5;i++){
    const angle=i*TAU/5+.12+.065*Math.sin(i*2+index);
    const length=1+.09*Math.sin(i*3+index),sweep=.045*Math.sin(i+index);
    const rotate=([x,y,z])=>[x*Math.cos(angle)-y*Math.sin(angle),x*Math.sin(angle)+y*Math.cos(angle),z];
    const point=(x,y)=>rotate([x*length,y+sweep*x*x,.025+p.opening*(.10*Math.sin(x*Math.PI)-.08*x*x)]);
    // Five independent slender, slightly irregular lance-shaped petals, with
    // generous negative space between them and no broad yellow throat ring.
    fan(petals,point(.49,0),[[.045,-.017],[.28,-.08],[.55,-.12],[.78,-.08],[1,.018],[.73,.095],[.43,.10],[.12,.028]].map(([x,y])=>point(x,y)));
    fan(throat,rotate([.09,0,.038]),[rotate([.035,-.02,.025]),rotate([.17,-.025,.035]),rotate([.20,0,.04]),rotate([.12,.03,.035])]);
    fan(calyx,rotate([.11,0,-.055]),[rotate([.025,-.055,-.075]),rotate([.24,0,-.02]),rotate([.025,.055,-.075])]);
  }
  // Small cream center: intentionally no large strawberry receptacle/anther ring.
  fan(center,[0,0,.09],Array.from({length:10},(_,i)=>[.065*Math.cos(i*TAU/10),.065*Math.sin(i*TAU/10),.025]));
  const tilt=p.tilt*Math.PI/180,az=azimuth*Math.PI/180;
  for(const mesh of [petals,throat,center,calyx]){
    for(let i=0;i<mesh.positions.length;i+=3){
      const [x,y,z]=mesh.positions.slice(i,i+3),yy=y*Math.cos(tilt)+z*Math.sin(tilt),zz=-y*Math.sin(tilt)+z*Math.cos(tilt);
      mesh.positions.splice(i,3,...add(anchor,mul([x*Math.cos(az)+zz*Math.sin(az),yy,-x*Math.sin(az)+zz*Math.cos(az)],radius*p.flowerSize)));
    }
    parts.push(mesh);
  }
}
export function gilleniaGeometry(spec,p){
  const stem=part('Stem and branches','stemColor'),leaves=part('Leaves','leafColor'),parts=[stem,leaves],heads=[];
  const root=[0,-1.2,0],joint=[p.bend,-.1,0],top=[spec.heads[0][0]+p.bend,spec.heads[0][1]*p.height,0];
  tube(stem,root,joint,.012);tube(stem,joint,top,.009);
  const mainAt=y=>y<=joint[1]?mix(root,joint,(y-root[1])/(joint[1]-root[1])):mix(joint,top,(y-joint[1])/(top[1]-joint[1]));
  for(const [index,[x,y,z,radius,azimuth]]of spec.heads.entries()){
    const anchor=[x+p.bend,y*p.height,z];
    if(index){
      const branch=mainAt(Math.max(-.95,anchor[1]-.65));
      const elbow=add(mix(branch,anchor,.6),[0,-.07,.025]);
      tube(stem,branch,elbow,.008);tube(stem,elbow,anchor,.006);
    }
    headGeometry(parts,index,anchor,radius,azimuth,p);
    heads.push({id:index,anchor,label:`花头 ${index+1}`});
  }
  for(let i=0;i<4;i++){
    const sign=i%2?-1:1,start=mainAt(-.95+i*.37),size=p.leafSize;
    const palm=add(start,[sign*.08*size,.035,.055]);tube(stem,start,palm,.006);
    for(const [angle,length]of [[-.85,.45],[0,.59],[.85,.45]]){
      const a=sign*1.02+angle,end=add(palm,[Math.sin(a)*length*size,Math.cos(a)*length*size,.045]);
      leaflet(leaves,palm,end,.064*size);
    }
  }
  return {parts,heads};
}
