// Original, deterministic web-only meshes. No downloaded assets or native
// species/animation integration. +Z is forward; groups use local wing hinges.
const TAU=Math.PI*2;
const add=(a,b)=>a.map((v,i)=>v+b[i]),sub=(a,b)=>a.map((v,i)=>v-b[i]),mul=(a,s)=>a.map(v=>v*s);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const unit=v=>mul(v,1/(Math.hypot(...v)||1));
export const animalCatalog=[
  {id:'bee',label:'蜜蜂 · 网页模型',description:'琥珀条纹腹部、六足、双触角、两对不透明浅色翅；停留 / 拍翼展示，不是游戏飞行控制。',
    colors:{bodyColor:'#d7a33f',backColor:'#483522',bellyColor:'#efd29a',wingColor:'#dae7df',accentColor:'#211c1b'}},
  {id:'kingfisher',label:'小鸟 · 三趾翠鸟',description:'三趾翠鸟 Ceyx erithaca：紫粉冠、橙红长喙、金黄腹、白喉颈斑、蓝黑叠羽和短尾；按参考照片重建的低模，非写实扫描。仅网页。',
    colors:{bodyColor:'#cb61ae',backColor:'#202936',bellyColor:'#ffc448',wingColor:'#4168d8',accentColor:'#f65b2c',throatColor:'#fff0d7',cheekColor:'#f69b32'},
    colorLabels:{bodyColor:'头冠 / 腰尾 · 紫粉',backColor:'背部 / 翼底 · 蓝黑',bellyColor:'胸腹 · 金黄',wingColor:'羽斑 / 耳斑 · 亮蓝',accentColor:'长喙 / 三趾足 · 橙红',throatColor:'喉部 / 颈侧斑 · 米白',cheekColor:'脸颊 / 额缘 · 橙黄'}},
];
export function animalGeometry(id){
  if(!animalCatalog.some(spec=>spec.id===id))throw new Error(`Unknown animal: ${id}`);
  const meshes=[],hinges=id==='bee'?{leftWing:[-.24,.21,.05],rightWing:[.24,.21,.05]}:{leftWing:[-.30,.23,-.06],rightWing:[.30,.23,-.06]};
  function mesh(name,color,group='body'){
    const out={name,color,group,positions:[],indices:[]};meshes.push(out);
    return {data:out,v(p){out.positions.push(...p);return out.positions.length/3-1;},tri(a,b,c){const point=i=>out.positions.slice(i*3,i*3+3);if(Math.hypot(...cross(sub(point(b),point(a)),sub(point(c),point(a))))>1e-10)out.indices.push(a,b,c);},quad(a,b,c,d){this.tri(a,b,c);this.tri(a,c,d);}};
  }
  function ellipsoid(name,center,radii,color,group='body',range=[0,1],rings=8,sides=12){
    const m=mesh(name,color,group),rows=[];
    // Latitude axis Z makes the bee's stripes cross its abdomen rather than
    // forming stacked waistbands around Y. Every band owns real surface area.
    for(let r=0;r<=rings;r++){
      const theta=(range[0]+(range[1]-range[0])*r/rings)*Math.PI;
      rows.push(Array.from({length:sides},(_,i)=>m.v(add(center,[radii[0]*Math.sin(theta)*Math.cos(i*TAU/sides),radii[1]*Math.sin(theta)*Math.sin(i*TAU/sides),radii[2]*Math.cos(theta)]))));
    }
    for(let r=0;r<rings;r++)for(let i=0;i<sides;i++)m.quad(rows[r][i],rows[r+1][i],rows[r+1][(i+1)%sides],rows[r][(i+1)%sides]);
  }
  function tube(name,points,radius,color,group='body'){
    const m=mesh(name,color,group);
    for(let i=0;i<points.length-1;i++){
      const a=points[i],b=points[i+1],axis=unit(sub(b,a)),reference=Math.abs(axis[2])<.9?[0,0,1]:[0,1,0],u=unit(cross(axis,reference)),v=cross(axis,u);
      const rings=[a,b].map(p=>Array.from({length:6},(_,j)=>m.v(add(p,add(mul(u,Math.cos(j*TAU/6)*radius),mul(v,Math.sin(j*TAU/6)*radius))))));
      for(let j=0;j<6;j++)m.quad(rings[0][j],rings[1][j],rings[1][(j+1)%6],rings[0][(j+1)%6]);
      const ca=m.v(a),cb=m.v(b);for(let j=0;j<6;j++){m.tri(ca,rings[0][(j+1)%6],rings[0][j]);m.tri(cb,rings[1][j],rings[1][(j+1)%6]);}
    }
  }
  function feather(name,outline,color,group='body',camber=.045){
    const m=mesh(name,color,group),center=outline.reduce((a,p)=>add(a,mul(p,1/outline.length)),[0,0,0]);center[1]+=camber;
    const c=m.v(center),rim=outline.map(p=>m.v(p));for(let i=0;i<rim.length;i++)m.tri(c,rim[i],rim[(i+1)%rim.length]);
  }
  // Continuous skin with anatomical color regions, not overlapping colored
  // balls. Shared analytic normals keep the plumage boundaries smooth.
  function plumage(name,profiles,region,sides=24){
    const parts=new Map();
    const point=(r,i)=>{const [y,w,d,z]=profiles[r],a=i*TAU/sides;return [w*Math.cos(a),y,z+d*Math.sin(a)];};
    function vertex(part,r,i){
      i%=sides;const key=`${r}:${i}`;
      if(part.vertices.has(key))return part.vertices.get(key);
      const index=part.m.v(point(r,i)),[,w,d]=profiles[r],a=i*TAU/sides;
      const lower=profiles[Math.max(0,r-1)],upper=profiles[Math.min(profiles.length-1,r+1)];
      const tangent=[(upper[1]-lower[1])*Math.cos(a),upper[0]-lower[0],upper[3]-lower[3]+(upper[2]-lower[2])*Math.sin(a)];
      const normal=w&&d?unit(cross(tangent,[-w*Math.sin(a),0,d*Math.cos(a)])):[0,r===0?-1:1,0];
      part.m.data.normals.push(...normal);part.vertices.set(key,index);return index;
    }
    for(let r=0;r<profiles.length-1;r++)for(let i=0;i<sides;i++){
      const color=region((profiles[r][0]+profiles[r+1][0])/2,(i+.5)*TAU/sides);
      if(!parts.has(color)){const m=mesh(`${name} / ${color}`,color);m.data.normals=[];parts.set(color,{m,vertices:new Map()});}
      const part=parts.get(color);part.m.quad(vertex(part,r,i),vertex(part,r+1,i),vertex(part,r+1,i+1),vertex(part,r,i+1));
    }
  }
  // A closed, gently convex feather. Thin parts still use the same opaque
  // coverage contract as the other models; there are no transparent decals.
  function plume(name,outline,color,group='body',normal=[0,1,0],bulge=.025,warp=p=>p,detail=0){
    const m=mesh(name,color,group),center=outline.reduce((a,p)=>add(a,mul(p,1/outline.length)),[0,0,0]),vertices=new Map();
    const vertex=p=>{
      const key=p.map(v=>v.toFixed(9)).join(':');
      if(!vertices.has(key))vertices.set(key,m.v(add(warp(p.slice(0,3)),mul(normal,p[3]))));
      return vertices.get(key);
    };
    const surface=(a,b,c,level)=>{
      if(!level){m.tri(vertex(a),vertex(b),vertex(c));return;}
      const ab=mul(add(a,b),.5),bc=mul(add(b,c),.5),ca=mul(add(c,a),.5);
      surface(a,ab,ca,level-1);surface(ab,b,bc,level-1);surface(ca,bc,c,level-1);surface(ab,bc,ca,level-1);
    };
    for(let i=0;i<outline.length;i++){
      const j=(i+1)%outline.length,a=[...outline[i],.004],b=[...outline[j],.004],c=[...outline[i],-.004],d=[...outline[j],-.004];
      // Subdivide before applying the flank warp. A single large fan cuts
      // through a convex body; sampling the skin keeps the folded cover solid.
      surface([...center,bulge],a,b,detail);surface([...center,-.012],d,c,detail);
      // Match the warped cap's subdivided rim on the side walls as well;
      // otherwise curved caps and straight walls leave T-junctions / gaps.
      const steps=2**detail,lerp=(a,b,t)=>add(mul(a,1-t),mul(b,t));
      for(let k=0;k<steps;k++)m.quad(vertex(lerp(a,b,k/steps)),vertex(lerp(c,d,k/steps)),vertex(lerp(c,d,(k+1)/steps)),vertex(lerp(a,b,(k+1)/steps)));
    }
  }
  if(id==='bee'){
    for(let band=0;band<7;band++)ellipsoid(`Abdomen stripe ${band+1}`,[0,-.03,-.48],[.44,.36,.64],band%2?'backColor':'bodyColor','body',[band/7,(band+1)/7],2,14);
    ellipsoid('Fuzzy thorax',[0,.02,.13],[.40,.39,.42],'bodyColor');
    ellipsoid('Bee head',[0,.05,.62],[.34,.30,.29],'backColor');
    ellipsoid('Bee forehead',[0,.18,.74],[.21,.13,.13],'bodyColor');
    for(const side of [-1,1]){
      ellipsoid(`Compound eye ${side}`,[side*.25,.10,.81],[.115,.15,.10],'eye');
      ellipsoid(`Eye glint ${side}`,[side*.27,.17,.885],[.028,.034,.025],'shine','body',[0,1],4,6);
      const antenna=[[side*.15,.26,.70],[side*.23,.53,.81],[side*.31,.60,.92]];
      tube(`Antenna ${side}`,antenna,.023,'accentColor');ellipsoid(`Antenna tip ${side}`,antenna[2],[.04,.045,.04],'accentColor','body',[0,1],4,6);
      for(let leg=0;leg<3;leg++){
        const z=.30-leg*.34;
        tube(`Leg ${side}:${leg}`,[[side*.27,-.17,z],[side*.54,-.32,z-.06],[side*.58,-.58,z+.10],[side*.65,-.60,z+.20]],.028,'accentColor');
      }
      const group=side<0?'leftWing':'rightWing';
      feather(`Forewing ${side}`,[[0,0,.10],[side*.42,.03,.42],[side*.98,.02,.36],[side*1.22,0,.08],[side*1.07,-.01,-.18],[side*.37,0,-.22]],'wingColor',group,.07);
      feather(`Hindwing ${side}`,[[0,-.025,-.05],[side*.48,.0,-.15],[side*.88,-.01,-.42],[side*.68,-.04,-.66],[side*.22,-.02,-.49]],'bellyColor',group,.045);
      tube(`Wing vein ${side}`,[[side*.10,.03,.06],[side*.55,.065,.05],[side*1.04,.04,.09]],.013,'bellyColor',group);
    }
  }else{
    const bodyProfiles=[
      [-.53,0,0,-.18],[-.46,.21,.25,-.18],[-.32,.34,.37,-.13],[-.10,.41,.43,-.08],
      [.10,.39,.41,-.04],[.24,.32,.34,0],[.34,.27,.29,.03],[.44,.18,.22,.07],[.51,0,0,.10],
    ];
    plumage('Kingfisher body',bodyProfiles,(y,a)=>{
      const front=Math.sin(a);
      if(front<-.15){if(y<-.34)return 'cheekColor';return front<-.87&&y<.24?'wingColor':'backColor';}
      if(y>.34&&front>.88)return 'throatColor';
      return y>.24?'cheekColor':'bellyColor';
    });
    plumage('Kingfisher head',[
      [.23,0,0,.20],[.32,.25,.27,.20],[.36,.29,.29,.20],[.40,.325,.316,.20],
      [.44,.346,.335,.20],[.48,.364,.346,.20],[.52,.376,.35,.198],[.56,.386,.35,.193],
      [.60,.39,.348,.189],[.64,.388,.34,.185],[.69,.38,.33,.18],[.74,.362,.316,.17],
      [.79,.34,.30,.16],[.87,.27,.25,.15],[.93,.16,.16,.14],[.96,0,0,.14],
    ],(y,a)=>{
      const front=Math.sin(a),side=Math.abs(Math.cos(a));
      if(y<.55&&front>(y<.41?.94:.86))return 'throatColor';
      if(((y-.375)/.037)**2+((front+.32)/.48)**2<1&&side>.55)return 'throatColor';
      if(((y-.51)/.075)**2+((front+.32)/.42)**2<1&&side>.65)return 'wingColor';
      if(y>.66&&y<.74&&front>.96)return 'backColor';
      if(y<.63+.04*front||y<.79&&front>.94)return 'cheekColor';
      return 'bodyColor';
    },32);
    // Warm rufous feather tips on the pink/lilac crown, blended in linear
    // light by the adapter. These are palette weights, not baked photo colors.
    const crown=meshes.find(part=>part.name==='Kingfisher head / bodyColor');
    crown.colorBlend={key:'cheekColor',weights:crown.positions.filter((_,i)=>i%3===1).map((y,i)=>{
      const x=crown.positions[i*3],z=crown.positions[i*3+2],top=Math.max(0,(y-.78)/.18);
      const grain=.5+.5*Math.sin(Math.atan2(x,z-.15)*12+y*65);
      return Math.min(.68,top*(.40+.16*grain)+Math.max(0,.15-z)*.35);
    })};
    // A broad-based, dorsoventrally flattened dagger, almost as long as the
    // head. Upper ridge and lower mandible have real volume and a fine seam.
    const bill=mesh('Kingfisher long bill','accentColor');
    const sections=[[.49,.60,.13,.075,.055],[.66,.596,.115,.063,.050],[1.03,.586,.057,.033,.026],[1.37,.575,0,0,0]];
    const rings=sections.map(([z,y,w,up,down])=>[[0,y+up,z],[w,y+.01,z],[w*.82,y-down,z],[-w*.82,y-down,z],[-w,y+.01,z]].map(p=>bill.v(p)));
    for(let r=0;r<rings.length-1;r++)for(let i=0;i<5;i++)bill.quad(rings[r][i],rings[r+1][i],rings[r+1][(i+1)%5],rings[r][(i+1)%5]);
    const cap=bill.v([0,.60,.49]);for(let i=0;i<5;i++)bill.tri(cap,rings[0][(i+1)%5],rings[0][i]);
    for(const side of [-1,1]){
      tube(`Bill seam ${side}`,[[side*.131,.593,.51],[side*.116,.59,.66],[side*.058,.582,1.03],[side*.007,.575,1.32]],.0035,'backColor');
      // Rotate the eye domes onto the head surface instead of attaching big
      // side-facing spheres. The ring, dark iris and glint are opaque meshes.
      const eyeCenter=[side*.337,.665,.358],angle=side*1.08,normal=[Math.sin(angle),0,Math.cos(angle)];
      for(const [name,radii,offset,color]of [['Eye surround',[.079,.080,.025],0,'backColor'],['Bird eye',[.067,.069,.028],.006,'eye']]){
        ellipsoid(`${name} ${side}`,[0,0,0],radii,color,'body',[0,1],6,12);
        const positions=meshes.at(-1).positions;
        for(let i=0;i<positions.length;i+=3){const [x,y,z]=positions.slice(i,i+3),p=add(eyeCenter,add([x*Math.cos(angle)+z*Math.sin(angle),y,-x*Math.sin(angle)+z*Math.cos(angle)],mul(normal,offset)));positions.splice(i,3,...p);}
      }
      ellipsoid(`Eye glint ${side}`,add(eyeCenter,add(mul(normal,.036),[0,.026,.007])),[.012,.013,.012],'shine','body',[0,1],4,8);
      tube(`Bird leg ${side}`,[[side*.17,-.43,-.02],[side*.18,-.59,.06],[side*.18,-.62,.06]],.022,'accentColor');
      // Three toes per foot: two forward, one opposing hind toe (not three
      // front toes, as on the retired generic bird recipe).
      for(const toe of [-1,1])tube(`Toe ${side}:front${toe}`,[[side*.18,-.62,.06],[side*.18+toe*.055,-.63,.20],[side*.18+toe*.070,-.665,.25]],.015,'accentColor');
      tube(`Toe ${side}:rear`,[[side*.18,-.62,.06],[side*.23,-.635,-.12],[side*.23,-.665,-.17]],.015,'accentColor');
      const group=side<0?'leftWing':'rightWing',normalOut=[side,0,0];
      const outline=points=>points.map(([x,y,z])=>[side*x,y,z]);
      // Folded feathers follow the same body's curved flank. A constant-X
      // paddle would sink into the belly in the middle and float at its rim.
      const wrapWing=([x,y,z])=>{
        const worldY=y+hinges[group][1],worldZ=z+hinges[group][2];
        let r=bodyProfiles.findIndex(p=>p[0]>=worldY);r=Math.max(1,r<0?bodyProfiles.length-1:r);
        const a=bodyProfiles[r-1],b=bodyProfiles[r],t=Math.max(0,Math.min(1,(worldY-a[0])/(b[0]-a[0])));
        const w=a[1]+(b[1]-a[1])*t,d=a[2]+(b[2]-a[2])*t,centerZ=a[3]+(b[3]-a[3])*t;
        const flank=d?w*Math.sqrt(Math.max(0,1-((worldZ-centerZ)/d)**2)):0;
        return [side*(Math.max(.06,flank)-.30+.012+Math.abs(x)*.15),y,z];
      };
      // Authored folded wing: shield-shaped shoulder, parallel primaries,
      // three overlapping rows of blue-tipped coverts on a blue-black base.
      plume(`Folded wing ${side}`,outline([[0,.16,.12],[.07,.06,.22],[.10,-.13,.14],[.09,-.43,-.02],[0,-.68,-.54],[-.04,-.55,-.62],[-.055,-.19,-.50],[-.04,.08,-.25]]),'backColor',group,normalOut,.015,wrapWing,2);
      for(let i=0;i<5;i++){
        const y=-.025-i*.016,z=.13-i*.070,x=.075-i*.005;
        plume(`Primary feather ${side}:${i}`,outline([[x,y+.03,z],[x,y,z+.055],[x+.015,-.64+i*.012,-.43-i*.032],[x,-.68+i*.012,-.48-i*.032],[x,y-.025,z-.03]]),'backColor',group,normalOut,.014,wrapWing,2);
        plume(`Primary blue edge ${side}:${i}`,outline([[x+.10,y-.045,z-.028],[x+.10,y-.08,z-.024],[x+.10,-.59+i*.012,-.425-i*.032],[x+.10,-.60+i*.012,-.443-i*.032]]),'wingColor',group,normalOut,.008,wrapWing,1);
      }
      for(let row=0;row<3;row++)for(let i=0;i<4;i++){
        const x=.11+row*.008-i*.009,y=.08-row*.105-i*.012,z=.105-i*.105-row*.020;
        plume(`Blue covert ${side}:${row}:${i}`,outline([[x,y+.035,z+.025],[x,y+.018,z-.050],[x+.01,y-.035,z-.055],[x+.01,y-.055,z+.008],[x,y-.015,z+.047]]),'wingColor',group,normalOut,.016,wrapWing,1);
      }
    }
    for(let i=-1;i<=1;i++){
      const x=i*.065;
      plume(`Short tail ${i}`,[[x-.06,-.38,-.42],[x+.06,-.38,-.42],[x+.052,-.61,-.75],[x,-.63,-.79],[x-.052,-.61,-.75]],'bodyColor');
      plume(`Rufous tail tip ${i}`,[[x-.052,-.589,-.69],[x+.052,-.589,-.69],[x+.045,-.613,-.75],[x,-.635,-.79],[x-.045,-.613,-.75]],'cheekColor','body',[0,1,0],.009);
    }
  }
  return {meshes,hinges};
}
export function animalPose(id,time,clip=0,spread=1){
  if(!animalCatalog.some(spec=>spec.id===id))throw new Error(`Unknown animal: ${id}`);
  const duration=id==='bee'?1:2,t=((Number.isFinite(time)?time:0)%duration+duration)%duration;
  spread=Math.max(.5,Math.min(1.3,Number.isFinite(spread)?spread:1));
  const flight=clip===1,bee=id==='bee',wave=Math.sin(t*TAU*(bee?8:3));
  // The kingfisher is authored with folded wings. The flight study unfolds
  // those hinges; the unanimated rest pose stays exactly at the authored pose.
  const angle=bee?(flight?.1+.8*wave:.15):flight?1.0+.65*wave:0;
  const amplitude=flight?spread:1;
  return {rootY:flight?.045*Math.sin(t*TAU/duration):0,rootPitch:flight?-.12:0,leftWing:-angle*amplitude,rightWing:angle*amplitude};
}
