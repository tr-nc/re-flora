// Shared art recipe for the preview and its derived game render mesh.
// All positions are in preview units; the game scales them to a two-voxel radius.
export function appleGeometry(){
  const parts=[];
  {
    const segments=24,rings=16,positions=[],indices=[];
    positions.push(0,.86-.25,0);
    for(let ring=1;ring<rings;ring++){
      const theta=Math.PI*ring/rings,vertical=Math.cos(theta),r=Math.pow(Math.sin(theta),.82);
      for(let segment=0;segment<segments;segment++){
        const angle=2*Math.PI*segment/segments,lobes=Math.cos(5*angle);
        const radius=r*(1+.09*vertical+.045*lobes*Math.max(vertical,0)**4);
        const shoulder=.045*lobes*r*Math.max(vertical,0)**4;
        const topDent=.25*Math.exp(-((r/.38)**2))*Math.max(vertical,0)**2;
        const bottomDent=.11*Math.exp(-((r/.3)**2))*Math.max(-vertical,0)**2;
        positions.push(radius*Math.cos(angle),.86*vertical+shoulder-topDent+bottomDent,radius*Math.sin(angle));
      }
    }
    const bottom=positions.length/3;positions.push(0,-.86+.11,0);
    for(let segment=0;segment<segments;segment++){
      const next=(segment+1)%segments;
      indices.push(0,1+next,1+segment);
      for(let ring=0;ring<rings-2;ring++){
        const a=1+ring*segments+segment,b=1+ring*segments+next;
        indices.push(a,b,a+segments,b,b+segments,a+segments);
      }
      indices.push(bottom,bottom-segments+segment,bottom-segments+next);
    }
    parts.push({name:'Apple',material:0,positions,indices});
  }
  {
    const positions=[],indices=[],segments=10;
    // Same cylinder as Three's CylinderGeometry(.027,.046,.43,10) at
    // (.025,.81,0) with rotation.z = -.13.
    for(let i=0;i<=segments;i++){
      const a=2*Math.PI*i/segments,s=Math.sin(a),c=Math.cos(a);
      for(const [radius,y] of [[.027,.215],[.046,-.215]]){
        const x=radius*s,yr=y*Math.cos(-.13)+x*Math.sin(-.13)+.81;
        positions.push(x*Math.cos(.13)-y*Math.sin(-.13)+.025,yr,radius*c);
      }
      if(i<segments){const k=2*i;indices.push(k,k+2,k+1,k+2,k+3,k+1);}
    }
    // Stem caps close its silhouette at the apple's recessed socket.
    for(const [radius,y,up] of [[.027,.215,true],[.046,-.215,false]]){
      const center=positions.length/3;
      positions.push(.025-y*Math.sin(-.13),.81+y*Math.cos(-.13),0);
      for(let i=0;i<segments;i++){
        const a=(i+1)%segments,b=i,ringA=2*a+(up?0:1),ringB=2*b+(up?0:1);
        indices.push(center,up?ringA:ringB,up?ringB:ringA);
      }
    }
    parts.push({name:'Stem',material:1,positions,indices});
  }
  {
    const positions=[],indices=[],steps=5;
    for(let i=0;i<=steps;i++){
      const t=i/steps,width=.14*Math.sin(Math.PI*t);
      const x=.055+.62*t,y=.91+.21*Math.sin(Math.PI*t)+.08*t,z=.04+.24*t;
      positions.push(x,y,z-width,x,y,z+width);
      if(i<steps){const a=2*i;indices.push(a,a+1,a+2,a+1,a+3,a+2);}
    }
    parts.push({name:'Apple leaf',material:2,positions,indices});
  }
  return parts;
}
