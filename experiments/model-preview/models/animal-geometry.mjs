// Original, deterministic web-only meshes. No downloaded assets or native
// species/animation integration. +Z is forward; groups use local wing hinges.
const TAU=Math.PI*2;
const add=(a,b)=>a.map((v,i)=>v+b[i]),sub=(a,b)=>a.map((v,i)=>v-b[i]),mul=(a,s)=>a.map(v=>v*s);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const unit=v=>mul(v,1/(Math.hypot(...v)||1));
export const animalCatalog=[
  {id:'bee',label:'蜜蜂 · 网页模型',description:'琥珀条纹腹部、六足、双触角、两对不透明浅色翅；停留 / 拍翼展示，不是游戏飞行控制。',
    colors:{bodyColor:'#d7a33f',backColor:'#483522',bellyColor:'#efd29a',wingColor:'#dae7df',accentColor:'#211c1b'}},
  {id:'sparrow',label:'小鸟 · 圆身麻雀',description:'短喙、圆胸、棕色翼斑与短扇尾；停栖 / 拍翼展示。仅网页，不含寻路或游戏行为。',
    colors:{bodyColor:'#967149',backColor:'#624a39',bellyColor:'#ead6ae',wingColor:'#ac8758',accentColor:'#c79957'}},
  {id:'swallow',label:'小鸟 · 长尾燕子',description:'深蓝背、浅胸、赤褐喉、尖长翼和分叉长尾；停栖 / 拍翼展示。仅网页。',
    colors:{bodyColor:'#335266',backColor:'#233647',bellyColor:'#eee6cb',wingColor:'#416882',accentColor:'#b96649'}},
];
export function animalGeometry(id){
  if(!animalCatalog.some(spec=>spec.id===id))throw new Error(`Unknown animal: ${id}`);
  const meshes=[],hinges=id==='bee'?{leftWing:[-.24,.21,.05],rightWing:[.24,.21,.05]}:{leftWing:[-.31,.22,-.02],rightWing:[.31,.22,-.02]};
  function mesh(name,color,group='body'){
    const out={name,color,group,positions:[],indices:[]};meshes.push(out);
    return {v(p){out.positions.push(...p);return out.positions.length/3-1;},tri(a,b,c){const point=i=>out.positions.slice(i*3,i*3+3);if(Math.hypot(...cross(sub(point(b),point(a)),sub(point(c),point(a))))>1e-10)out.indices.push(a,b,c);},quad(a,b,c,d){this.tri(a,b,c);this.tri(a,c,d);}};
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
  function beak(center,width,length,color){
    const m=mesh('Tapered beak',color),base=[[-width,0,0],[0,width*.65,0],[width,0,0],[0,-width*.5,0]].map(p=>m.v(add(center,p))),tip=m.v(add(center,[0,0,length]));
    for(let i=0;i<4;i++)m.tri(base[i],base[(i+1)%4],tip);m.tri(base[0],base[2],base[1]);m.tri(base[0],base[3],base[2]);
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
    const swallow=id==='swallow',body=swallow?[.35,.47,.48]:[.48,.54,.42],head=swallow?[.31,.29,.29]:[.38,.34,.32];
    ellipsoid('Bird body',[0,0,-.04],body,'bodyColor');
    ellipsoid('Cream breast',[0,-.02,.25],swallow?[.28,.38,.22]:[.38,.43,.24],'bellyColor');
    ellipsoid('Bird head',[0,.48,.27],head,'bodyColor');
    if(!swallow)ellipsoid('Sparrow cheek',[0,.42,.49],[.31,.20,.16],'bellyColor');
    else ellipsoid('Swallow throat',[0,.29,.49],[.20,.14,.115],'accentColor');
    beak([0,.43,.55],swallow?.08:.115,swallow?.19:.24,'accentColor');
    for(const side of [-1,1]){
      ellipsoid(`Bird eye ${side}`,[side*(swallow?.23:.27),.56,.48],[.067,.074,.066],'eye','body',[0,1],6,8);
      ellipsoid(`Eye glint ${side}`,[side*(swallow?.24:.28),.59,.531],[.019,.023,.018],'shine','body',[0,1],4,6);
      tube(`Bird leg ${side}`,[[side*.17,-.39,.02],[side*.17,-.66,.06]],.027,'accentColor');
      for(let toe=-1;toe<=1;toe++)tube(`Toe ${side}:${toe}`,[[side*.17,-.66,.06],[side*.17+toe*.09,-.68,.23-Math.abs(toe)*.025]],.020,'accentColor');
      const group=side<0?'leftWing':'rightWing',span=swallow?1.18:.87;
      feather(`Wing silhouette ${side}`,[[0,0,.17],[side*.35,.07,.24],[side*span,.01,swallow?-.72:-.35],[side*span*.62,-.025,-.58],[side*.18,-.02,-.44],[0,0,-.15]],'backColor',group,.065);
      for(let featherIndex=0;featherIndex<3;featherIndex++){
        const z=.10-featherIndex*.16;
        feather(`Wing feather ${side}:${featherIndex}`,[[side*.08,.08,z],[side*.33,.095,z+.06],[side*span*(.72+featherIndex*.09),.05,z-(swallow?.52:.28)],[side*.37,.05,z-.11]],featherIndex===1?'bodyColor':'wingColor',group,.018);
      }
      if(swallow)feather(`Fork tail ${side}`,[[side*.04,-.21,-.34],[side*.23,-.24,-.32],[side*.47,-.54,-1.25],[side*.13,-.43,-.82]],'backColor');
    }
    if(!swallow)for(let featherIndex=-1;featherIndex<=1;featherIndex++)feather(`Fan tail ${featherIndex}`,[[featherIndex*.12-.09,-.24,-.28],[featherIndex*.12+.09,-.24,-.28],[featherIndex*.22+.11,-.39,-.85],[featherIndex*.22-.11,-.39,-.85]],featherIndex===0?'wingColor':'backColor');
  }
  return {meshes,hinges};
}
export function animalPose(id,time,clip=0,spread=1){
  if(!animalCatalog.some(spec=>spec.id===id))throw new Error(`Unknown animal: ${id}`);
  const duration=id==='bee'?1:2,t=((Number.isFinite(time)?time:0)%duration+duration)%duration;
  spread=Math.max(.5,Math.min(1.3,Number.isFinite(spread)?spread:1));
  const flight=clip===1,bee=id==='bee',wave=Math.sin(t*TAU*(bee?8:3));
  const angle=bee?(flight?.1+.8*wave:.15):flight?.15+.65*wave:-1.05;
  const amplitude=flight?spread:1;
  return {rootY:flight?.045*Math.sin(t*TAU/duration):0,rootPitch:flight?-.12:0,leftWing:-angle*amplitude,rightWing:angle*amplitude};
}
