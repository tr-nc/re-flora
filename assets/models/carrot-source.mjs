// Original low-poly first-year cultivated carrot. Y-up, soil at y=0.
// One tapered storage root and seven pinnate leaves in a basal rosette;
// no flowering umbel (a second-year reproductive state).
const TAU=Math.PI*2;
export function carrotGeometry(){
  const parts=[];
  const mesh=name=>{const p={name,material:'palette',head:0,positions:[],uvs:[],indices:[]};parts.push(p);return p;};
  const vertex=(p,v,uv)=>{p.positions.push(...v);p.uvs.push(...uv);return p.positions.length/3-1;};
  const tri=(p,a,b,c)=>p.indices.push(a,b,c);
  const root=mesh('Tapered carrot storage root');
  // A rounded exposed shoulder and a taper largely below the soil plane.
  const profile=[[-.78,.014],[-.62,.052],[-.42,.105],[-.18,.158],[.04,.185],[.15,.15],[.19,.055]];
  const rows=profile.map(([y,r],j)=>Array.from({length:12},(_,i)=>{
    const a=i*TAU/12,rr=r*(1+.035*Math.sin(i*2.1+j));
    return vertex(root,[Math.cos(a)*rr,y,Math.sin(a)*rr],[.32,.08+.75*j/(profile.length-1)]);
  }));
  for(let j=0;j<rows.length-1;j++)for(let i=0;i<12;i++){
    const k=(i+1)%12;tri(root,rows[j][i],rows[j+1][i],rows[j+1][k]);tri(root,rows[j][i],rows[j+1][k],rows[j][k]);
  }
  const bottom=vertex(root,[0,-.81,0],[.32,.08]),top=vertex(root,[0,.19,0],[.32,.85]);
  for(let i=0;i<12;i++){const k=(i+1)%12;tri(root,bottom,rows[0][i],rows[0][k]);tri(root,top,rows.at(-1)[k],rows.at(-1)[i]);}
  // Subtle broken transverse root scars, not a noisy phototexture.
  const scars=mesh('Root growth rings');
  for(const y of [.015,.07,.115])for(let i=0;i<5;i++){
    const a=i*TAU/5+y*8,r=.183-(y>.07?.012:0);
    const ids=[[-.14,-.004],[.14,-.004],[.14,.004],[-.14,.004]].map(([da,dy])=>vertex(scars,[Math.cos(a+da)*r,y+dy,Math.sin(a+da)*r],[.32,.1]));
    tri(scars,...ids.slice(0,3));tri(scars,ids[0],ids[2],ids[3]);
  }
  const leaves=mesh('Feathery basal rosette');
  // Exaggerated petiole thickness preserves a connected silhouette after the
  // shared 20px surface bake; anatomical hairlines disappear at game scale.
  for(let i=0;i<8;i++){
    const a=i*TAU/8,b=(i+1)*TAU/8;
    const ids=[[Math.cos(a)*.075,.12,Math.sin(a)*.075],
      [Math.cos(b)*.075,.12,Math.sin(b)*.075],[0,.34,0]].map(v=>vertex(leaves,v,[.87,.75]));
    tri(leaves,...ids);
  }
  for(let leaf=0;leaf<7;leaf++){
    const a=leaf*2.3999632297,spread=.44+.13*Math.sin(leaf*1.7),height=.78+.16*Math.cos(leaf*1.9);
    const point=t=>[Math.cos(a)*spread*t*t,.18+height*t,Math.sin(a)*spread*t*t];
    const side=[-Math.sin(a),0,Math.cos(a)];
    const uv=leaf%2?[.87,.75]:[.87,.25]; // palette D/C: two greens
    const blade=(base,tip,width)=>{
      const mid=base.map((v,i)=>v*.45+tip[i]*.55);
      const ids=[base,mid.map((v,i)=>v+side[i]*width),tip,mid.map((v,i)=>v-side[i]*width)]
        .map(v=>vertex(leaves,v,uv));
      tri(leaves,ids[0],ids[1],ids[2]);tri(leaves,ids[0],ids[2],ids[3]);
    };
    for(let j=0;j<6;j++)blade(point(j/6),point((j+1)/6),.052);
    for(let j=1;j<=5;j++)for(const sign of [-1,1]){
      const t=.20+j*.12,base=point(t),length=.28*(1-t)+.06;
      const tip=point(t+.12).map((v,i)=>v+side[i]*length*sign);
      blade(base,tip,.045);
      // Secondary divisions give each frond a fern-like rather than grass-blade silhouette.
      for(const branch of [.4,.7]){
        const b=base.map((v,i)=>v+(tip[i]-v)*branch);
        for(const s of [-1,1]){
          const end=b.map((v,i)=>v+(tip[i]-base[i])*.27+(i===1?s*.038:0));
          blade(b,end,.035);
        }
      }
    }
  }
  return {parts,heads:[{id:0,anchor:[0,0,0],label:'Carrot root and rosette'}],socketNormal:[0,1,0]};
}
