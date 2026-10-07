// Original web-only geometry fitted to the user's side-profile photograph.
// +Z faces the bill. The body is a rearward pear, not a vertical chest under
// a spherical head. Profile rows are [Y, lateral radius, front Z, rear Z].
export const kingfisherProfiles={
  body:[
    [-.98,0,-.72,-.72],[-.91,.25,-.18,-1.26],[-.79,.40,.025,-1.25],
    [-.61,.52,.27,-1.18],[-.38,.62,.49,-1.195],[-.16,.65,.61,-1.16],
    [.10,.62,.69,-1.04],[.32,.50,.735,-.81],[.50,.36,.712,-.50],
    [.62,.24,.48,-.35],[.70,0,.15,.15],
  ],
  head:[
    [.39,0,.42,.42],[.46,.20,.73,-.16],[.57,.34,.83,-.36],
    [.72,.43,.80,-.43],[.87,.42,.75,-.40],[1.02,.34,.66,-.30],
    [1.11,.25,.60,-.18],[1.17,.145,.45,-.03],[1.20,0,.21,.21],
  ],
};
const TAU=Math.PI*2,add=(a,b)=>a.map((v,i)=>v+b[i]),sub=(a,b)=>a.map((v,i)=>v-b[i]),mul=(a,s)=>a.map(v=>v*s);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],unit=v=>mul(v,1/(Math.hypot(...v)||1));
const clamp=(x,a=0,b=1)=>Math.max(a,Math.min(b,x));
export function kingfisherProfileAt(profiles,y){
  y=clamp(y,profiles[0][0],profiles.at(-1)[0]);
  let r=profiles.findIndex(p=>p[0]>=y);r=Math.max(1,r);
  const a=profiles[r-1],b=profiles[r],before=profiles[Math.max(0,r-2)],after=profiles[Math.min(profiles.length-1,r+1)];
  const h=b[0]-a[0],t=(y-a[0])/h;
  const out=[y];
  for(let i=1;i<4;i++){
    const m0=(b[i]-before[i])/(b[0]-before[0]),m1=(after[i]-a[i])/(after[0]-a[0]);
    out.push((2*t**3-3*t*t+1)*a[i]+(t**3-2*t*t+t)*h*m0+(-2*t**3+3*t*t)*b[i]+(t**3-t*t)*h*m1);
  }
  out[1]=Math.max(0,out[1]);return out;
}
export function kingfisherWingUV(y,z){
  const dy=y-.54,dz=z-.24;
  return [.5+(-.758*dy+.652*dz)/1.05,(-.652*dy-.758*dz)/1.875];
}
export function kingfisherGeometry(){
  const meshes=[],hinges={leftWing:[-.36,.48,-.12],rightWing:[.36,.48,-.12]};
  function mesh(name,color,group='body',appearance){
    const out={name,color,group,positions:[],indices:[],...(appearance?{appearance,uvs:[]}: {})};meshes.push(out);
    return {out,v(p,uv){out.positions.push(...p);if(appearance)out.uvs.push(...uv);return out.positions.length/3-1;},
      tri(a,b,c){const p=i=>out.positions.slice(i*3,i*3+3);if(Math.hypot(...cross(sub(p(b),p(a)),sub(p(c),p(a))))>1e-10)out.indices.push(a,b,c);},
      quad(a,b,c,d){this.tri(a,b,c);this.tri(a,c,d);}};
  }
  function skinPoint(profiles,y,a){
    const [,w,front,rear]=kingfisherProfileAt(profiles,y),depth=Math.max(0,(front-rear)/2),center=(front+rear)/2;
    const fluff=1+.006*Math.sin(a*17+y*43)+.003*Math.sin(a*31-y*29);
    return [w*Math.cos(a)*fluff,y,center+depth*Math.sin(a)*fluff];
  }
  function skin(name,profiles,color,appearance){
    const m=mesh(name,color,'body',appearance),rows=[],sides=40,y0=profiles[0][0],y1=profiles.at(-1)[0];m.out.normals=[];
    const ys=profiles.slice(0,-1).flatMap((p,i)=>[p[0],(p[0]+profiles[i+1][0])/2]);ys.push(y1);
    for(const y of ys){
      const row=[];
      for(let i=0;i<=sides;i++){
        const a=i*TAU/sides,p=skinPoint(profiles,y,a);row.push(m.v(p,[i/sides,(y-y0)/(y1-y0)]));
        const normal=y===y0?[0,-1,0]:y===y1?[0,1,0]:unit(cross(sub(skinPoint(profiles,y+.0001,a),skinPoint(profiles,y-.0001,a)),sub(skinPoint(profiles,y,a+.0001),skinPoint(profiles,y,a-.0001))));
        m.out.normals.push(...normal);
      }rows.push(row);
    }
    for(let r=0;r<rows.length-1;r++)for(let i=0;i<sides;i++)m.quad(rows[r][i],rows[r+1][i],rows[r+1][i+1],rows[r][i+1]);
  }
  function tube(name,points,radius,color,curved=false){
    const m=mesh(name,color),path=[];
    for(let i=0;i<points.length-1;i++)for(let k=0;k<(curved?3:1);k++){
      const t=k/3,a=points[Math.max(0,i-1)],b=points[i],c=points[i+1],d=points[Math.min(points.length-1,i+2)];
      path.push(curved?b.map((v,j)=>.5*((2*v)+(-a[j]+c[j])*t+(2*a[j]-5*v+4*c[j]-d[j])*t*t+(-a[j]+3*v-3*c[j]+d[j])*t**3)):b);
    }path.push(points.at(-1));
    const rings=path.map((p,i)=>{
      const axis=unit(sub(path[Math.min(i+1,path.length-1)],path[Math.max(0,i-1)]));
      const u=unit(cross(axis,Math.abs(axis[2])<.9?[0,0,1]:[0,1,0])),v=cross(axis,u);
      return Array.from({length:8},(_,j)=>m.v(add(p,add(mul(u,Math.cos(j*TAU/8)*radius),mul(v,Math.sin(j*TAU/8)*radius)))));
    });
    for(let i=0;i<rings.length-1;i++)for(let j=0;j<8;j++)m.quad(rings[i][j],rings[i+1][j],rings[i+1][(j+1)%8],rings[i][(j+1)%8]);
    const a=m.v(path[0]),b=m.v(path.at(-1));for(let j=0;j<8;j++){m.tri(a,rings[0][(j+1)%8],rings[0][j]);m.tri(b,rings.at(-1)[j],rings.at(-1)[(j+1)%8]);}
  }
  function dome(name,center,radii,normal,color){
    const m=mesh(name,color),u=unit(cross([0,1,0],normal)),v=unit(cross(normal,u)),rows=[];
    for(let r=0;r<=6;r++){
      const t=r*Math.PI/6;
      rows.push(Array.from({length:16},(_,i)=>{const a=i*TAU/16;return m.v(add(center,add(add(mul(u,Math.sin(t)*Math.cos(a)*radii[0]),mul(v,Math.sin(t)*Math.sin(a)*radii[1])),mul(normal,Math.cos(t)*radii[2]))));}));
    }
    for(let r=0;r<6;r++)for(let i=0;i<16;i++)m.quad(rows[r][i],rows[r+1][i],rows[r+1][(i+1)%16],rows[r][(i+1)%16]);
  }
  // Closed, subdivided sheets. UVs stay in the authored profile plane while
  // the skin warp supplies real lateral curvature; caps and walls share rims.
  function sheet(name,outline,color,group,appearance,normal,warp,uv,bulge=.02,detail=2){
    const m=mesh(name,color,group,appearance),center=outline.reduce((a,p)=>add(a,mul(p,1/outline.length)),[0,0,0]),cache=new Map();
    const vertex=p=>{const key=p.map(v=>v.toFixed(9)).join(':');if(!cache.has(key))cache.set(key,m.v(add(warp(p.slice(0,3)),mul(normal,p[3])),uv(p)));return cache.get(key);};
    function tri(a,b,c,n){
      if(!n){m.tri(vertex(a),vertex(b),vertex(c));return;}
      const ab=mul(add(a,b),.5),bc=mul(add(b,c),.5),ca=mul(add(c,a),.5);tri(a,ab,ca,n-1);tri(ab,b,bc,n-1);tri(ca,bc,c,n-1);tri(ab,bc,ca,n-1);
    }
    const lerp=(a,b,t)=>add(mul(a,1-t),mul(b,t));
    for(let i=0;i<outline.length;i++){
      const j=(i+1)%outline.length,a=[...outline[i],.004],b=[...outline[j],.004],c=[...outline[i],-.004],d=[...outline[j],-.004];
      tri([...center,bulge],a,b,detail);tri([...center,-.015],d,c,detail);
      const steps=2**detail;for(let k=0;k<steps;k++)m.quad(vertex(lerp(a,b,k/steps)),vertex(lerp(c,d,k/steps)),vertex(lerp(c,d,(k+1)/steps)),vertex(lerp(a,b,(k+1)/steps)));
    }
  }
  skin('Kingfisher body',kingfisherProfiles.body,'bellyColor','body');
  skin('Kingfisher head',kingfisherProfiles.head,'bodyColor','head');
  const bill=mesh('Kingfisher long bill','accentColor','body','bill');
  const sections=[[.73,.81,.16,.14,.090],[.94,.795,.148,.108,.074],[1.38,.775,.085,.052,.038],[1.84,.755,0,0,0]];
  const rings=sections.map(([z,y,w,up,down])=>{const p=[[0,y+up,z],[w,y+.005,z],[w*.85,y-down,z],[-w*.85,y-down,z],[-w,y+.005,z]];p.push(p[0]);return p.map((p,i)=>bill.v(p,[i/5,(z-.73)/1.11]));});
  for(let r=0;r<rings.length-1;r++)for(let i=0;i<5;i++)bill.quad(rings[r][i],rings[r+1][i],rings[r+1][i+1],rings[r][i+1]);
  const cap=bill.v([0,.81,.73],[.5,0]);for(let i=0;i<5;i++)bill.tri(cap,rings[0][i+1],rings[0][i]);
  for(const side of [-1,1]){
    const normal=unit([side*.94,.06,.33]),eye=[side*.395,.81,.445];
    dome(`Eye surround ${side}`,eye,[.101,.091,.016],normal,'iris');
    dome(`Bird eye ${side}`,add(eye,mul(normal,.007)),[.079,.078,.017],normal,'eye');
    dome(`Eye glint ${side}`,add(eye,add(mul(normal,.026),[0,.028,.021])),[.011,.012,.009],normal,'shine');
    tube(`Bill seam ${side}`,[[side*.155,.80,.75],[side*.144,.786,.94],[side*.081,.768,1.38],[side*.008,.755,1.78]],.003,'billSeam');
    // Short tucked tarsi and grasping toes, not straight stilts / walking feet.
    tube(`Bird leg ${side}`,[[side*.15,-.75,-.26],[side*.16,-.89,-.08],[side*.16,-.94,-.04]],.026,'accentColor',true);
    for(const toe of [-1,1])tube(`Toe ${side}:front${toe}`,[[side*.16,-.94,-.04],[side*.16+toe*.033,-.975,.07],[side*.16+toe*.039,-1.045,.08],[side*.16+toe*.034,-1.085,.025]],.018,'accentColor',true);
    tube(`Toe ${side}:rear`,[[side*.16,-.94,-.04],[side*.19,-.98,-.18],[side*.19,-1.07,-.16],[side*.18,-1.10,-.095]],.018,'accentColor',true);
    const group=side<0?'leftWing':'rightWing',hinge=hinges[group];
    const wrap=([offset,y,z])=>{
      const [,w,front,rear]=kingfisherProfileAt(kingfisherProfiles.body,y),d=(front-rear)/2,c=(front+rear)/2;
      const x=d?w*Math.sqrt(Math.max(0,1-((z-c)/d)**2)):0;
      return [side*(Math.max(.055,x)+offset)-hinge[0],y-hinge[1],z-hinge[2]];
    };
    const outline=[[.015,.54,.29],[.015,.43,.43],[.015,.10,.33],[.015,-.24,.015],[.015,-.50,-.50],[.015,-.66,-1.13],[.015,-.67,-1.20],[.015,-.45,-1.18],[.015,-.20,-1.11],[.015,.10,-.91],[.015,.43,-.48]];
    sheet(`Folded wing ${side}`,outline,'backColor',group,'wing',[side,0,0],wrap,p=>kingfisherWingUV(p[1],p[2]),.02,2);
    for(let i=0;i<5;i++){
      const y=.16-i*.025,z=-.30-i*.085,ty=-.66+i*.025,tz=-1.18+i*.052;
      sheet(`Primary feather ${side}:${i}`,[[.035,y+.025,z+.042],[.035,y-.018,z+.057],[.035,ty,tz+.040],[.035,ty-.025,tz],[.035,y,z-.03]],'backColor',group,'wing',[side,0,0],wrap,p=>kingfisherWingUV(p[1],p[2]),.014,2);
    }
  }
  for(let i=-1;i<=1;i++){
    const x=i*.062;
    sheet(`Short tail ${i}`,[[x-.055,-.85,-1.18],[x+.055,-.85,-1.18],[x+.042,-.955,-1.33],[x,-.97,-1.37],[x-.042,-.955,-1.33]],'bodyColor','body','tail',[0,1,0],p=>p,p=>[.5+(p[0]-x)/.12,(-1.18-p[2])/.19],.012,1);
  }
  return {meshes,hinges};
}
