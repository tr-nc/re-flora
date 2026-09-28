// Original, deterministic low-poly authoring recipes. No downloaded meshes,
// textures, random state, browser APIs or alternate rendering pipeline.
const TAU=Math.PI*2;
const base={height:1,flowerSize:1,opening:1,tilt:38,leafSize:1,bend:.08,
  petalColor:'#d695c5',innerColor:'#be78a6',centerColor:'#ecd18a',leafColor:'#638d53',stemColor:'#52734a'};
export const flowerCatalog=[
  {id:'wild-geranium',displayName:'Wild Geranium',stemLayers:41,label:'五瓣野花 · 野老鹳草',latin:'Geranium maculatum',kind:'radial',petals:5,width:.43,leaf:'palm',
    heads:[[-.27,.85,0,.44,-12],[.48,.5,-.08,.34,24]],defaults:{...base},note:'五片圆钝花瓣 · 掌状裂叶 · 两朵错高小花'},
  {id:'forget-me-not',displayName:'Forget-me-not',stemLayers:42,label:'勿忘草',latin:'Myosotis sylvatica',kind:'radial',petals:5,width:.45,leaf:'blade',cacheFamily:'open-radial',cacheTemplate:'cosmos',
    heads:[[-.4,.72,0,.3,-20],[.18,1,-.08,.31,12],[.49,.45,.1,.26,32]],
    defaults:{...base,petalColor:'#83b9eb',innerColor:'#e5ecda',centerColor:'#e7ba48',leafColor:'#719b63',tilt:28},note:'三朵蓝色五裂花 · 浅色喉圈与黄色小眼'},
  {id:'oxeye-daisy',displayName:'Oxeye Daisy',stemLayers:42,label:'白色滨菊',latin:'Leucanthemum vulgare',kind:'radial',petals:16,width:.145,leaf:'blade',
    heads:[[.02,.86,0,.64,0]],defaults:{...base,petalColor:'#f4f0da',innerColor:'#e5d9a9',centerColor:'#e4b53f',tilt:42},note:'十六条狭白瓣 · 扁黄色花盘'},
  {id:'cosmos',displayName:'Cosmos',stemLayers:42,label:'波斯菊',latin:'Cosmos bipinnatus',kind:'radial',petals:8,width:.39,leaf:'feather',notch:.1,cacheFamily:'open-radial',cacheTemplate:'cosmos',
    heads:[[-.18,.86,0,.56,-8],[.57,.28,-.08,.31,25]],
    defaults:{...base,petalColor:'#e4a0c0',innerColor:'#b94880',centerColor:'#e9be4c',leafColor:'#689467',tilt:44},note:'八片缺口宽瓣 · 深粉内圈 · 羽状细叶'},
  {id:'coneflower',displayName:'Coneflower',stemLayers:43,label:'紫松果菊',latin:'Echinacea purpurea',kind:'radial',petals:12,width:.2,leaf:'blade',droop:.75,
    heads:[[0,.92,0,.65,0]],defaults:{...base,petalColor:'#d18bb4',innerColor:'#b36c98',centerColor:'#b77c3c',leafColor:'#65854f',tilt:55},note:'十二条下垂粉瓣 · 高起的橙褐锥盘'},
  {id:'tulip',displayName:'Tulip',stemLayers:37,label:'郁金香',latin:'Tulipa',kind:'tulip',leaf:'broad',
    heads:[[0,.63,0,.46,0]],defaults:{...base,petalColor:'#e8a075',innerColor:'#f3c38e',centerColor:'#755048',leafColor:'#769b80',tilt:70},note:'六片交叠花被 · 高杯形 · 两片宽叶'},
];

const add=(a,b)=>a.map((v,i)=>v+b[i]);
const sub=(a,b)=>a.map((v,i)=>v-b[i]);
const mul=(a,n)=>a.map(v=>v*n);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const unit=a=>mul(a,1/(Math.hypot(...a)||1));
const mix=(a,b,t)=>a.map((v,i)=>v+(b[i]-v)*t);
function builder(name,material,head=null){
  return {name,material,head,positions:[],indices:[],
    vertex(p){this.positions.push(...p);return this.positions.length/3-1;},
    triangle(a,b,c){
      const point=i=>this.positions.slice(i*3,i*3+3),u=sub(point(b),point(a)),v=sub(point(c),point(a));
      if(Math.hypot(...cross(u,v))>1e-10)this.indices.push(a,b,c);
    },
    quad(a,b,c,d){this.triangle(a,b,c);this.triangle(a,c,d);}};
}
function tube(mesh,points,radius){
  for(let i=0;i<points.length-1;i++){
    const a=points[i],b=points[i+1],axis=unit(sub(b,a)),u=unit(cross(axis,[0,0,1])),v=cross(axis,u);
    const rings=[a,b].map(p=>Array.from({length:5},(_,j)=>mesh.vertex(add(p,add(mul(u,Math.cos(j*TAU/5)*radius),mul(v,Math.sin(j*TAU/5)*radius))))));
    for(let j=0;j<5;j++)mesh.quad(rings[0][j],rings[1][j],rings[1][(j+1)%5],rings[0][(j+1)%5]);
    mesh.triangle(rings[0][2],rings[0][1],rings[0][0]);mesh.triangle(rings[0][3],rings[0][2],rings[0][0]);mesh.triangle(rings[0][4],rings[0][3],rings[0][0]);
    mesh.triangle(rings[1][0],rings[1][1],rings[1][2]);mesh.triangle(rings[1][0],rings[1][2],rings[1][3]);mesh.triangle(rings[1][0],rings[1][3],rings[1][4]);
  }
}
function blade(mesh,start,end,width,fold=.08){
  const axis=sub(end,start),side=unit(cross(axis,[0,0,1]));
  const root=mesh.vertex(start),tip=mesh.vertex(end);
  const mid=add(mix(start,end,.47),[0,0,fold]);
  const c=mesh.vertex(mid),l=mesh.vertex(add(mix(start,end,.43),mul(side,width))),r=mesh.vertex(add(mix(start,end,.5),mul(side,-width)));
  mesh.triangle(root,l,c);mesh.triangle(root,c,r);mesh.triangle(l,tip,c);mesh.triangle(c,tip,r);
}
function lathe(mesh,rings,sides=10){
  const rows=rings.map(([radius,z])=>Array.from({length:sides},(_,i)=>mesh.vertex([radius*Math.cos(i*TAU/sides),radius*Math.sin(i*TAU/sides),z])));
  for(let i=0;i<rows.length-1;i++)for(let j=0;j<sides;j++)mesh.quad(rows[i][j],rows[i][(j+1)%sides],rows[i+1][(j+1)%sides],rows[i+1][j]);
}
function petals(outer,inner,spec,p){
  const count=spec.petals??6;
  for(let i=0;i<count;i++){
    const angle=i*TAU/count+.12,ca=Math.cos(angle),sa=Math.sin(angle);
    const point=(t,w)=>{
      const cup=(spec.cup??.16)*p.opening*t*t;
      const droop=(spec.droop??0)*p.opening*t*t;
      const radial=.16+t*(.84-(spec.cup?.1:0));
      const x=radial,y=w*spec.width*Math.sin(Math.PI*t*.91);
      return [x*ca-y*sa,x*sa+y*ca,cup-droop+.04*Math.abs(w)+.035*Math.sin(i*2+t*4)*t];
    };
    // Central ridge plus broad faceted sides; six perimeter samples per petal.
    const rows=[0,.23,.72,1].map(t=>[-1,0,1].map(w=>{
      const q=point(t,w);if(t===1&&w===0&&spec.notch){q[0]-=ca*spec.notch;q[1]-=sa*spec.notch;}return q;
    }));
    for(let r=0;r<3;r++){
      const mesh=r===0?inner:outer;
      const a=rows[r].map(v=>mesh.vertex(v)),b=rows[r+1].map(v=>mesh.vertex(v));
      for(let k=0;k<2;k++)mesh.quad(a[k],b[k],b[k+1],a[k+1]);
    }
  }
}
function tulip(outer,inner,p){
  for(let i=0;i<6;i++){
    const angle=i*TAU/6,rows=[];
    for(const [r,z,w]of [[.1,-.16,.1],[.62,.16,.48],[.8,.65,.5],[.6*p.opening,1.08,.28]]){
      rows.push([-1,0,1].map(side=>{
        const a=angle+side*w;return outer.vertex([r*Math.cos(a),r*Math.sin(a),z+(side===0?.09:0)+(i%2)*.05]);
      }));
    }
    for(let j=0;j<3;j++)for(let k=0;k<2;k++)outer.quad(rows[j][k],rows[j][k+1],rows[j+1][k+1],rows[j+1][k]);
  }
  lathe(inner,[[0,-.12],[.24,-.08],[.5,.13]],10);
}

export function flowerGeometry(id,settings={}){
  const spec=flowerCatalog.find(item=>item.id===id);if(!spec)throw new Error(`Unknown flower: ${id}`);
  const p={...spec.defaults,...settings},parts=[],heads=[];
  const stem=builder('Stem and branches','stemColor'),leaves=builder('Leaves','leafColor');parts.push(stem,leaves);
  const root=[0,-1.2,0],joint=[p.bend,-.15,0];
  const top=[spec.heads[0][0]+p.bend,spec.heads[0][1]*p.height,0];
  tube(stem,[root,[p.bend*.3,-.7,0],joint,top],.018);
  for(const [index,head]of spec.heads.entries()){
    const [x,y,z,radius,azimuth]=head,anchor=[x+p.bend,y*p.height,z],scale=radius*p.flowerSize;
    if(index)tube(stem,[joint,mix(joint,anchor,.55),anchor],.013);
    const outer=builder(`Flower ${index+1} petals`,'petalColor',index),inner=builder(`Flower ${index+1} throat`,'innerColor',index),core=builder(`Flower ${index+1} center`,'centerColor',index),calyx=builder(`Flower ${index+1} calyx`,'stemColor',index);
    if(spec.kind==='tulip')tulip(outer,inner,p);
    else petals(outer,inner,spec,p);
    const dome=spec.id==='coneflower';
    lathe(core,dome?[[0,.02],[.3,.04],[.27,.28],[.17,.46],[0,.54]]:[[0,0],[.22,.012],[.2,.09],[0,.12]],10);
    lathe(calyx,[[0,-.14],[.22,-.04],[.26,0],[0,.012]],5);
    const tilt=p.tilt*Math.PI/180,az=azimuth*Math.PI/180;
    const transform=([a,b,c])=>{
      const yy=b*Math.cos(tilt)+c*Math.sin(tilt),zz=-b*Math.sin(tilt)+c*Math.cos(tilt);
      return add(anchor,mul([a*Math.cos(az)+zz*Math.sin(az),yy,-a*Math.sin(az)+zz*Math.cos(az)],scale));
    };
    const flowerParts=[outer,inner,core,calyx];
    for(const part of flowerParts){for(let i=0;i<part.positions.length;i+=3)part.positions.splice(i,3,...transform(part.positions.slice(i,i+3)));parts.push(part);}
    heads.push({id:index,anchor,label:`花头 ${index+1}`});
  }
  const count=spec.leaf==='broad'?2:4;
  for(let i=0;i<count;i++){
    const sign=i%2?-1:1,t=(i+1)/(count+1),start=mix(root,joint,t);
    const length=p.leafSize*(spec.leaf==='broad'?.95:.6);
    const end=add(start,[sign*length*.7,length*(spec.leaf==='broad'?1.3:.48),.12+(i%2)*.09]);
    tube(stem,[start,mix(start,end,.24)],.009);
    if(spec.leaf==='palm'){
      const palm=mix(start,end,.35);tube(stem,[start,palm],.01);
      for(let j=0;j<5;j++){
        const a=(-.85+j*.42)*sign;blade(leaves,palm,add(palm,[Math.sin(a)*length*.7+sign*.13,Math.cos(a)*length*.6,.04]),length*.13);
      }
    }else if(spec.leaf==='feather'||spec.leaf==='lobed'){
      tube(stem,[start,end],.008);
      for(let j=1;j<5;j++)for(const side of [-1,1]){
        const from=mix(start,end,j/6),to=add(from,[sign*length*.2,length*side*.22,.03]);
        blade(leaves,from,to,length*(spec.leaf==='feather'?.035:.1));
      }
    }else blade(leaves,start,end,length*(spec.leaf==='broad'?.26:.17),spec.leaf==='broad'?.16:.06);
  }
  return {parts:parts.map(({name,material,head,positions,indices})=>({name,material,head,positions,indices})),heads};
}
