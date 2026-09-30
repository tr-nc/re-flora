// One head-only surface family. Presets are data, never per-species mesh code.
// Deliberately scoped for demos, not a universal botanical model: one radial
// whorl repeats the same petal shape/pose, with a simple disk/dome/cone center.
// Tulips are open-cup approximations, not interleaved whorls or closed buds.
// More petals still share one whorl; this does not model layered double flowers
// or tightly wrapped rose centers. Separate petals cannot form a fused bell/
// tube corolla, and identical radial repeats cannot express orchid lip/column
// roles or bilateral structure. Wide/cupped petals may intersect; palette maps
// change color only, not these structural limits. Accept these limits for the
// current demos rather than adding more topology families or species branches.
const TAU=Math.PI*2;
export const flowerShapeDefaults={flowerSize:1,petalCount:5,petalLength:1,petalWidth:.42,tipSharpness:.15,notch:0,opening:.12,centerRadius:.19,centerHeight:.12,centerShape:'dome',tilt:35};
export function normalizeFlowerShape(input={}){
  const p={...flowerShapeDefaults,...input};
  for(const [key,lo,hi]of [['flowerSize',.65,1.3],['petalCount',3,24],['petalLength',.6,1.4],['petalWidth',.08,.65],['tipSharpness',0,1],['notch',0,.25],['opening',-1,1],['centerRadius',.04,.35],['centerHeight',0,.55],['tilt',-15,85]]){
    const v=Number(p[key]);p[key]=Math.min(hi,Math.max(lo,Number.isFinite(v)?v:flowerShapeDefaults[key]));
  }
  p.petalCount=Math.round(p.petalCount);
  if(!['flat','dome','cone'].includes(p.centerShape))p.centerShape='dome';
  return Object.fromEntries(Object.keys(flowerShapeDefaults).map(key=>[key,p[key]]));
}
export function flowerUV(island,u,v){
  // Half-texel-safe gutters even in the minimum supported 32x32 weight map.
  return island==='petal'?[.025+u*.70,.025+v*.95]:[.775+u*.20,(island==='center'?.025:.525)+v*.45];
}
export function parametricFlower(input={}){
  const p=normalizeFlowerShape(input),parts=[];
  const tilt=p.tilt*Math.PI/180,scale=.53*p.flowerSize;
  const transform=([x,y,z])=>[x*scale,(y*Math.cos(tilt)+z*Math.sin(tilt))*scale,(-y*Math.sin(tilt)+z*Math.cos(tilt))*scale];
  function mesh(name,island){
    const out={name,material:'palette',head:0,positions:[],uvs:[],indices:[]};parts.push(out);
    return {vertex(point,uv){out.positions.push(...transform(point));out.uvs.push(...flowerUV(island,...uv));return out.positions.length/3-1;},
      triangle(a,b,c){const point=i=>out.positions.slice(i*3,i*3+3),pa=point(a),pb=point(b).map((v,i)=>v-pa[i]),pc=point(c).map((v,i)=>v-pa[i]);
        if(Math.hypot(pb[1]*pc[2]-pb[2]*pc[1],pb[2]*pc[0]-pb[0]*pc[2],pb[0]*pc[1]-pb[1]*pc[0])>1e-10)out.indices.push(a,b,c);},
      quad(a,b,c,d){this.triangle(a,b,c);this.triangle(a,c,d);}};
  }
  // Integrate the radial tangent: positive pose creates a cup, zero is flat,
  // negative pose hangs back. Tip shape/width stay independent of this pose.
  const path=t=>{
    const theta=p.opening*1.8,theta0=theta*.30,delta=theta*.70;
    return Math.abs(delta)<1e-6?[p.centerRadius*.65+p.petalLength*t,0]:[
      p.centerRadius*.65+p.petalLength*(Math.sin(theta0+delta*t)-Math.sin(theta0))/delta,
      p.petalLength*(Math.cos(theta0)-Math.cos(theta0+delta*t))/delta];
  };
  const petals=mesh('Flower petals','petal'),segments=12,across=4;
  for(let petal=0;petal<p.petalCount;petal++){
    const angle=petal*TAU/p.petalCount+.12,rows=[];
    for(let i=0;i<=segments;i++){
      const t=i/segments,row=[];
      for(let j=0;j<=across;j++){
        const w=j/across*2-1,et=t-p.notch*Math.pow(t,8)*(1-Math.abs(w));
        const [radius,z]=path(et);
        const width=p.petalWidth*Math.pow(Math.max(0,Math.sin(Math.PI*t*(1-p.notch*.35))),.45+p.tipSharpness*1.6);
        const lateral=w*width;
        row.push(petals.vertex([radius*Math.cos(angle)-lateral*Math.sin(angle),radius*Math.sin(angle)+lateral*Math.cos(angle),z+.06*Math.abs(w)*Math.sin(Math.PI*t)],[(w+1)/2,t]));
      }rows.push(row);
    }
    for(let i=0;i<segments;i++)for(let j=0;j<across;j++)petals.quad(rows[i][j],rows[i+1][j],rows[i+1][j+1],rows[i][j+1]);
  }
  function disk(builder,radius,height,profile,zBase){
    const rings=6,sides=24,rows=[];
    for(let ring=0;ring<=rings;ring++){
      const r=ring/rings;
      const z=zBase+height*(profile==='flat'?0:profile==='cone'?1-r:Math.sqrt(Math.max(0,1-r*r)));
      rows.push(Array.from({length:sides},(_,i)=>{
        const x=Math.cos(i*TAU/sides)*r,y=Math.sin(i*TAU/sides)*r;
        return builder.vertex([x*radius,y*radius,z],[x*.5+.5,y*.5+.5]);
      }));
    }
    for(let ring=0;ring<rings;ring++)for(let i=0;i<sides;i++)builder.quad(rows[ring][i],rows[ring+1][i],rows[ring+1][(i+1)%sides],rows[ring][(i+1)%sides]);
  }
  disk(mesh('Flower center','center'),p.centerRadius,p.centerHeight,p.centerShape,.015);
  const calyx=mesh('Flower calyx','calyx');
  disk(calyx,p.centerRadius*1.22,-.12,'dome',-.025);
  return {parts,heads:[{id:0,anchor:[0,0,0],label:'完整花头'}]};
}
