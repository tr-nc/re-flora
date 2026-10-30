// Shared procedural mesh: callers supply the same vendored Three.js geometry primitives.
// No renderer, DOM, image assets or game state is owned here.
import {makePlant,point,add,mul} from './blackberry-structure.mjs';
export const blackberryColors={old:'#765347',fresh:'#6e7f46',twig:'#7e7251',prickle:'#af8470',leaf0:'#344f35',leaf1:'#49653a',leaf2:'#627d43',leaf3:'#72854b',leaf4:'#597444',green:'#819252',red:'#9d454c',ripe:'#302637',ripeLight:'#514255',petal:'#eee6d9',heart:'#c3a35c',sepal:'#5d7143'};
export function blackberryMesh(THREE,options={}) {
const state={stage:'fruit',skeleton:false,...options};
const plantData=makePlant(state);
const vector=a=>new THREE.Vector3(...a);

const sphere=new THREE.IcosahedronGeometry(1,1);
const petalSphere=new THREE.IcosahedronGeometry(1,0);
const buckets={};
function collect(key,geometry) {
  // The common non-indexed position/normal layout allows all primitives to merge.
  let g=geometry.index?geometry.toNonIndexed():geometry;
  if(g!==geometry) geometry.dispose();
  for(const name of Object.keys(g.attributes)) if(!['position','normal'].includes(name))g.deleteAttribute(name);
  (buckets[key]??=[]).push(g);
}
function blob(key,position,scale,rotation=0,coarse=false) {
  const g=(coarse?petalSphere:sphere).clone();
  g.scale(...scale);g.rotateY(rotation);g.translate(...position);collect(key,g);
}
function tube(key,curve,radius,segments=18) {
  const c=new THREE.CubicBezierCurve3(...curve.map(vector));
  const g=new THREE.TubeGeometry(c,segments,radius,5,false);
  const positions=g.getAttribute('position');
  for(let i=0;i<=segments;i++) {
    const center=c.getPointAt(i/segments),shrink=1-.75*(i/segments);
    for(let j=0;j<=5;j++) {
      const k=i*6+j,p=new THREE.Vector3().fromBufferAttribute(positions,k).sub(center).multiplyScalar(shrink).add(center);
      positions.setXYZ(k,p.x,p.y,p.z);
    }
  }
  g.computeVertexNormals();collect(key,g);
}
function stem(key,a,b,radius=.003) {tube(key,[a,add(mul(a,2/3),mul(b,1/3)),add(mul(a,1/3),mul(b,2/3)),b],radius,3);}
function leafBlade(base,angle,tilt,length,tone) {
  const outline=[[0,0],[.18,.30],[.26,.24],[.33,.43],[.41,.35],[.48,.49],[.57,.36],[.64,.4],[.73,.26],[.8,.28],[1,0]];
  const vertices=[],indices=[];
  for(const [u,w] of outline) vertices.push(u*length,Math.sin(u*Math.PI)*length*.075,-w*length*.72);
  for(let i=outline.length-2;i>0;i--) {const [u,w]=outline[i];vertices.push(u*length,Math.sin(u*Math.PI)*length*.075,w*length*.72);}
  const count=vertices.length/3,center=count;vertices.push(length*.48,length*.14,0);
  for(let i=0;i<count;i++)indices.push(center,i,(i+1)%count);
  const g=new THREE.BufferGeometry();g.setAttribute('position',new THREE.Float32BufferAttribute(vertices,3));g.setIndex(indices);g.computeVertexNormals();
  g.rotateZ(tilt);g.rotateY(-angle);g.translate(...base);collect(`leaf${tone}`,g);
}
function leafCluster(spec) {
  const {base,angle,size,tilt,tone}=spec;
  const hub=add(base,[Math.cos(angle)*.075,.035,Math.sin(angle)*.075]);
  stem('twig',base,hub,.0028);
  for(let k=0;k<3;k++) {
    const a=angle+(k-1)*1.05;
    const start=add(hub,[Math.cos(a)*.025,0,Math.sin(a)*.025]);
    stem('twig',hub,start,.0017);
    leafBlade(start,a,tilt+(k===1?.15:-.12),size*(k===1?1.1:.85),(tone+k%2)%5);
  }
}
function fruitCluster(site,index) {
  const {base,angle,size}=site;
  for(let k=0;k<3;k++) {
    const a=angle+(k-1)*1.55;
    const top=add(base,[Math.cos(a)*.065,(k===1?.035:-.012),Math.sin(a)*.065]);
    stem('twig',base,top,.002);
    const ripe=(index+k)%5, key=ripe<3?'ripe':ripe===3?'red':'green';
    for(let l=0;l<4;l++) {
      const count=l===3?4:6,r=l===3?.010:.016;
      for(let j=0;j<count;j++) {
        const theta=j/count*Math.PI*2+(l%2)*.5;
        const p=add(top,[Math.cos(theta)*r*size,-(.012+l*.011)*size,Math.sin(theta)*r*size]);
        blob(key==='ripe'&&j%3===0?'ripeLight':key,p,[.010*size,.010*size,.010*size]);
      }
    }
    for(let j=0;j<5;j++) {
      const a=j*Math.PI*2/5;
      blob('sepal',add(top,[Math.cos(a)*.012,0,Math.sin(a)*.012]),[.015,.003,.005],-a,true);
    }
  }
}
function flowerCluster(site) {
  for(let k=0;k<3;k++) {
    const a=site.angle+(k-1)*1.8,center=add(site.base,[Math.cos(a)*.07,.025+k*.013,Math.sin(a)*.07]);
    stem('twig',site.base,center,.002);
    for(let j=0;j<5;j++) {
      const angle=j/5*Math.PI*2;
      blob('petal',add(center,[Math.cos(angle)*.023,0,Math.sin(angle)*.023]),[.026,.007,.017],-angle,true);
    }
    blob('heart',add(center,[0,.009,0]),[.011,.008,.011],0,true);
  }
}

  for(const cane of plantData.canes) {
    tube(cane.fresh?'fresh':'old',cane.curve,cane.radius);
    for(const lateral of cane.laterals)tube('twig',lateral.curve,.005,7);
    for(let j=1;j<18;j++) {
      const t=j/19,p=point(cane.curve,t),a=j*2.4+cane.id;
      const tip=add(p,[Math.cos(a)*.026,-.013,Math.sin(a)*.026]);
      stem('prickle',p,tip,.006*(1-t*.5));
    }
  }
  if(!state.skeleton) {
    for(const leaf of plantData.leaves)leafCluster(leaf);
    if(state.stage!=='leaf')plantData.fruitSites.forEach((site,i)=>state.stage==='fruit'?fruitCluster(site,i):flowerCluster(site));
  }

const parts=Object.entries(buckets).map(([material,geometries])=>{
  const positions=[],normals=[];
  for(const g of geometries){
    for(const v of g.getAttribute('position').array)positions.push(v);
    for(const v of g.getAttribute('normal').array)normals.push(v);
    g.dispose();
  }
  return {material,positions,normals};
});
sphere.dispose();petalSphere.dispose();
return {plantData,parts};
}
