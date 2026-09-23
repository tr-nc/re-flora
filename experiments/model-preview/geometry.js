import * as THREE from 'three';

// Clip in homogeneous coordinates BEFORE dividing by w: the same path works for
// both projections and cannot create unbounded coverage near the camera plane.
function clipTriangle(points){
  let polygon=points;
  for(const [axis,sign] of [[0,1],[0,-1],[1,1],[1,-1],[2,1],[2,-1]]){
    const output=[];
    for(let i=0;i<polygon.length;i++){
      const a=polygon[i],b=polygon[(i+1)%polygon.length];
      const da=a[3]+sign*a[axis],db=b[3]+sign*b[axis];
      if(da>=0)output.push(a);
      if((da>=0)!==(db>=0)){
        const t=da/(da-db);output.push(a.map((v,j)=>v+(b[j]-v)*t));
      }
    }
    polygon=output;if(!polygon.length)break;
  }
  return polygon;
}

export function projectGroups(asset,camera,size){
  camera.updateMatrixWorld(true);asset.scene.updateMatrixWorld(true);
  const pv=new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix,camera.matrixWorldInverse);
  const position=new THREE.Vector3(),clip=new THREE.Vector4();
  return asset.repairGroups.map(group=>{
    const triangles=[];
    for(const mesh of group.meshes){
      if(!mesh.visible)continue;
      const geometry=mesh.geometry,count=geometry.index?.count??geometry.attributes.position.count;
      const start=geometry.drawRange.start,end=Math.min(count,start+geometry.drawRange.count);
      const transform=new THREE.Matrix4().multiplyMatrices(pv,mesh.matrixWorld);
      for(let i=start;i+2<end;i+=3){
        const points=[0,1,2].map(j=>{
          mesh.getVertexPosition(geometry.index?geometry.index.getX(i+j):i+j,position);
          clip.set(position.x,position.y,position.z,1).applyMatrix4(transform);return clip.toArray();
        });
        const polygon=clipTriangle(points).filter(p=>p[3]>1e-9).map(p=>[(p[0]/p[3]*.5+.5)*size,(p[1]/p[3]*.5+.5)*size,p[2]/p[3]]);
        for(let j=1;j+1<polygon.length;j++)triangles.push([polygon[0],polygon[j],polygon[j+1]]);
      }
    }
    return {id:group.id,label:group.label,triangles};
  });
}
