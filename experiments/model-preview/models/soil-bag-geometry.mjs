// Preview-only art recipe: one gusseted pouch, one corner opening, absolute poses.
// No inventory/deposition simulation. Approval and native asset publishing are a later stage.
const TAU=Math.PI*2,HEIGHT=1.64,TOP=HEIGHT/2,TOP_WIDTH=.42,OPEN_X=.12,POWER=.55;
const clamp=(value,low,high)=>Math.max(low,Math.min(high,Number.isFinite(value)?value:low));
const smooth=(low,high,value)=>{const t=clamp((value-low)/(high-low),0,1);return t*t*(3-2*t);};
const signedPower=(value,power)=>Math.sign(value)*Math.abs(value)**power;
const cutoff=Math.acos((OPEN_X/TOP_WIDTH)**(1/POWER));
const angles=[...Array.from({length:32},(_,i)=>i*TAU/32),cutoff,TAU-cutoff].sort((a,b)=>a-b);
export const soilBagClips=[
  {name:'撕开一个袋口',duration:1.65,loop:false},
  {name:'袋口朝下 · 倾倒营养土',duration:3.8,loop:false},
];
export const soilBagDefaults={bagColor:'#829477',labelColor:'#f5e9c6',inkColor:'#304c3c',stickerDesign:'sprout',fullness:1,sideFlatness:0,labelScale:1};
export const soilBagPresets=[
  {name:'林下营养土',values:{...soilBagDefaults}},
  {name:'育苗基质',values:{...soilBagDefaults,bagColor:'#bd956d',labelColor:'#ffefcc',inkColor:'#63513e',stickerDesign:'layers',fullness:.85}},
  {name:'花园配方',values:{...soilBagDefaults,bagColor:'#b7786c',labelColor:'#f8dfd4',inkColor:'#743f50',stickerDesign:'flower',fullness:1.1}},
];

function part(name,material,group='bag'){
  const out={name,material,group,positions:[],indices:[],uvs:[]};
  return {out,v(p,uv=[0,0]){out.positions.push(...p);out.uvs.push(...uv);return out.positions.length/3-1;},tri(a,b,c){out.indices.push(a,b,c);},quad(a,b,c,d){this.tri(a,b,c);this.tri(a,c,d);}};
}
function polygon(mesh,points,up=false){
  const center=points.reduce((out,p)=>out.map((v,i)=>v+p[i]/points.length),[0,0,0]);
  const c=mesh.v(center),ring=points.map(p=>mesh.v(p));
  for(let i=0;i<ring.length;i++)up?mesh.tri(c,ring[(i+1)%ring.length],ring[i]):mesh.tri(c,ring[i],ring[(i+1)%ring.length]);
}
function box(mesh,min,max){
  const vertices=[[0,0,0],[1,0,0],[1,1,0],[0,1,0],[0,0,1],[1,0,1],[1,1,1],[0,1,1]].map(p=>mesh.v(p.map((t,i)=>min[i]+t*(max[i]-min[i]))));
  for(const face of [[0,3,2,1],[4,5,6,7],[0,4,7,3],[1,2,6,5],[3,7,6,2],[0,1,5,4]])mesh.quad(...face.map(i=>vertices[i]));
}
function surface(v,angle,opening,fullness,sideFlatness){
  const wave=Math.sin(Math.PI*v),width=.48+.035*wave-.06*v;
  const x=width*signedPower(Math.cos(angle),POWER);
  const rounded=wave**.68;
  // Fill the upper/lower shoulders to make the side walls flatter, independently
  // of peak thickness. Keep both rims EXACTLY unchanged, including sin(pi) residue.
  const flat=v===0||v===1?rounded:wave**.18;
  const profile=rounded+sideFlatness*(flat-rounded);
  const base=.018+(.15+.035*fullness)*profile;
  const aperture=opening*.135*smooth(.80,1,v)*smooth(OPEN_X,OPEN_X+.09,x);
  const crease=.009*wave*Math.sin(v*37+angle*3)*(.3+.7*Math.abs(Math.cos(angle))**4);
  const z=(base+aperture+crease)*signedPower(Math.sin(angle),POWER);
  return [x,v*HEIGHT-TOP,z];
}

export function soilBagGeometry({opening=0,fullness=1,sideFlatness=0,labelScale=1,fill=1}={}){
  opening=clamp(opening,0,1);fullness=clamp(fullness,.35,1.25);sideFlatness=clamp(sideFlatness,0,1);labelScale=clamp(labelScale,.7,1.08);fill=clamp(fill,0,1);
  const body=part('Soft pouch with gussets','bag'),rows=[];
  for(let r=0;r<=16;r++)rows.push(angles.map(angle=>body.v(surface(r/16,angle,opening,fullness,sideFlatness),[angle/TAU,r/16])));
  for(let r=0;r<16;r++)for(let i=0;i<angles.length;i++)body.quad(rows[r][i],rows[r+1][i],rows[r+1][(i+1)%angles.length],rows[r][(i+1)%angles.length]);
  polygon(body,angles.map(angle=>surface(0,angle,opening,fullness,sideFlatness)));

  // Only the left part stays sealed. The right polygon is the ONE real opening;
  // its front/back lips separate, instead of painting a black hole on a closed box.
  const topRing=angles.map(angle=>surface(1,angle,opening,fullness,sideFlatness));
  const fixed=part('Remaining sealed top','seam');
  polygon(fixed,topRing.filter(p=>p[0]<=OPEN_X+1e-8),true);
  box(fixed,[-TOP_WIDTH,TOP-.012,-.020],[OPEN_X,TOP+.042,.020]);
  const flap=part('Attached tear strip','seam','tab');
  box(flap,[0,-.015,-.029],[TOP_WIDTH-OPEN_X,.045,.029]);

  const mouth=topRing.filter(p=>p[0]>=OPEN_X-1e-8);
  // Order around the right perimeter (back lip -> right corner -> front lip).
  mouth.sort((a,b)=>Math.atan2(a[2],a[0]-OPEN_X)-Math.atan2(b[2],b[0]-OPEN_X));
  const lining=part('Inner paper at the opening','lining');
  const inner=mouth.map(p=>[p[0]*.98,p[1]-.13,p[2]*.9]);
  const rim=mouth.map(p=>lining.v(p)),lower=inner.map(p=>lining.v(p));
  for(let i=0;i<rim.length;i++)lining.quad(rim[i],rim[(i+1)%rim.length],lower[(i+1)%rim.length],lower[i]);
  const soil=part('Soil visible inside the bag','soil');
  polygon(soil,inner.map(p=>[p[0],p[1]-(1-fill)*.16,p[2]]),true);

  const sticker=part('Replaceable front sticker','sticker');
  const stickerRows=[];
  for(let r=0;r<=14;r++){
    const v=.235+r/14*.535*labelScale;
    stickerRows.push(Array.from({length:13},(_,i)=>{
      const normalizedX=(i/12*2-1)*.77*labelScale;
      const angle=Math.acos(signedPower(normalizedX,1/POWER));
      const p=surface(v,angle,opening,fullness,sideFlatness);p[2]+=.012;
      return sticker.v(p,[i/12,r/14]);
    }));
  }
  for(let r=0;r<14;r++)for(let i=0;i<12;i++)sticker.quad(stickerRows[r][i],stickerRows[r][i+1],stickerRows[r+1][i+1],stickerRows[r+1][i]);
  return {parts:[body.out,fixed.out,flap.out,lining.out,soil.out,sticker.out],tabHinge:[OPEN_X,TOP+.006,0],mouthLocal:[(OPEN_X+TOP_WIDTH)/2,TOP,0]};
}

export function soilBagPose(time,clip=0){
  clip=clip===1?1:0;
  const phase=clamp((Number.isFinite(time)?time:0)/soilBagClips[clip].duration,0,1);
  const opening=clip===0?smooth(.08,.85,phase):1;
  const tilt=clip===1?smooth(.04,.32,phase)*(1-smooth(.78,1,phase)):0;
  const angle=(clip===1?-2.5*tilt:-.025*Math.sin(phase*Math.PI))||0;
  const root=clip===1?[.10*Math.sin(phase*Math.PI),.24*tilt,0]:[0,.015*Math.sin(phase*Math.PI),0];
  const mouthNormal=[-Math.sin(angle),Math.cos(angle),0],local=[(OPEN_X+TOP_WIDTH)/2,TOP,0];
  const mouthWorld=[root[0]+local[0]*Math.cos(angle)-local[1]*Math.sin(angle),root[1]+local[0]*Math.sin(angle)+local[1]*Math.cos(angle),0];
  const flow=clip===1?Math.max(0,-mouthNormal[1]-.12)*smooth(.19,.29,phase)*(1-smooth(.71,.79,phase)):0;
  return {phase,opening,angle,root,tabAngle:2.05*smooth(.10,.9,clip===0?phase:1),mouthNormal,mouthWorld,flow,fill:clip===1?1-.4*smooth(.20,.77,phase):1};
}

// Coarse flow-direction placeholder, NOT a soil-clod particle simulation.
// It exits the actual world-space mouth and falls along world gravity, not the bag's rotated Y.
export function soilBagFlowGeometry(pose,time=0){
  const mesh=part('Simplified gravity flow','soil','flow'),rings=[];
  const length=.72,amount=clamp(pose.flow,0,1);
  for(let r=0;r<=8;r++){
    const t=r/8,exit=.10*t*Math.exp(-3*t);
    const center=pose.mouthWorld.map((v,i)=>v+pose.mouthNormal[i]*exit+(i===1?-length*t:0));
    const width=(.070-.029*t)*Math.sqrt(amount)*(1+.08*Math.sin(t*21+time*7));
    rings.push(Array.from({length:7},(_,i)=>{
      const angle=i*TAU/7;
      return mesh.v([center[0]+Math.cos(angle)*width,center[1],center[2]+Math.sin(angle)*width*.63]);
    }));
  }
  for(let r=0;r<8;r++)for(let i=0;i<7;i++)mesh.quad(rings[r][i],rings[r+1][i],rings[r+1][(i+1)%7],rings[r][(i+1)%7]);
  return mesh.out;
}
