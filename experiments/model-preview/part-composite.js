import * as THREE from 'three';
import {tileDepth} from './part-depth.mjs';

export const PART_COMPOSITE_SIZE=512;

// Cropping changes X/Y only. Head color and depth therefore use exactly the
// full plant's projection, even in perspective; no lookAt-per-flower parallax.
export function partCamera(camera,part){
  const center=part.center.clone().applyMatrix4(camera.matrixWorldInverse),r=part.span/2;
  const corners=[];
  for(const x of [-r,r])for(const y of [-r,r])for(const z of [-r,r]){
    const point=center.clone().add(new THREE.Vector3(x,y,z));
    if(camera.isPerspectiveCamera&&point.z>=-camera.near)throw new Error('Flower part crosses the camera near plane');
    corners.push(point.applyMatrix4(camera.projectionMatrix));
  }
  const minX=Math.min(...corners.map(p=>p.x)),maxX=Math.max(...corners.map(p=>p.x));
  const minY=Math.min(...corners.map(p=>p.y)),maxY=Math.max(...corners.map(p=>p.y));
  const span=Math.max(maxX-minX,maxY-minY),x=(minX+maxX)/2,y=(minY+maxY)/2;
  const crop=new THREE.Matrix4().set(2/span,0,0,-2*x/span,0,2/span,0,-2*y/span,0,0,1,0,0,0,0,1);
  const result=camera.clone();result.projectionMatrix.premultiply(crop);result.projectionMatrixInverse.copy(result.projectionMatrix).invert();
  return {camera:result,rect:{x,y,span}};
}

export class PartComposite{
  constructor(){
    this.scene=new THREE.Scene();this.camera=new THREE.Camera();this.entries=[];
    this.depthTarget=new THREE.WebGLRenderTarget(1,1);
    this.depthMaterial=new THREE.MeshDepthMaterial({depthPacking:THREE.RGBADepthPacking,side:THREE.DoubleSide});
  }
  readDepth(renderer,asset,camera,size){
    this.depthTarget.setSize(size,size);
    const previous=asset.scene.overrideMaterial,bytes=new Uint8Array(size*size*4);
    try{
      asset.scene.overrideMaterial=this.depthMaterial;
      renderer.setRenderTarget(this.depthTarget);renderer.render(asset.scene,camera);
      renderer.readRenderTargetPixels(this.depthTarget,0,0,size,size,bytes);
    }finally{asset.scene.overrideMaterial=previous;renderer.setRenderTarget(null);}
    return bytes;
  }
  entry(index,size){
    let entry=this.entries[index];
    if(entry?.size===size)return entry;
    if(entry)this.releaseEntry(entry);
    const color=new THREE.DataTexture(new Uint8Array(size*size*4),size,size);
    const depth=new THREE.DataTexture(new Float32Array(size*size),size,size,THREE.RedFormat,THREE.FloatType);
    for(const texture of [color,depth]){texture.minFilter=texture.magFilter=THREE.NearestFilter;texture.generateMipmaps=false;}
    const material=new THREE.ShaderMaterial({
      uniforms:{colorTile:{value:color},depthTile:{value:depth},rect:{value:new THREE.Vector3()}},
      depthTest:true,depthWrite:true,
      vertexShader:'uniform vec3 rect; varying vec2 tex; void main(){tex=uv;gl_Position=vec4(rect.xy+position.xy*rect.z*.5,0.,1.);}',
      fragmentShader:'uniform sampler2D colorTile; uniform sampler2D depthTile; varying vec2 tex; void main(){vec4 color=texture2D(colorTile,tex);if(color.a<.5)discard;gl_FragColor=color;gl_FragDepth=texture2D(depthTile,tex).r;}',
    });
    const mesh=new THREE.Mesh(new THREE.PlaneGeometry(2,2),material);mesh.frustumCulled=false;this.scene.add(mesh);
    entry={size,color,depth,material,mesh};this.entries[index]=entry;return entry;
  }
  render(renderer,asset,camera,size,renderTile){
    const visible=asset.meshes.map(mesh=>mesh.visible),tiles=[];
    try{
      for(const part of asset.pixelParts){
        const members=new Set(part.meshes);
        asset.meshes.forEach((mesh,i)=>{mesh.visible=visible[i]&&members.has(mesh);});
        const view=partCamera(camera,part),groups=asset.repairGroups.filter(group=>group.id===part.id);
        const subset={...asset,repairGroups:groups};
        const tile=renderTile(subset,view.camera);
        const packed=this.readDepth(renderer,subset,view.camera,size);
        const depth=tileDepth(tile.original,tile.rgba,packed,tile.projectedGroups,size);
        const entry=this.entry(tiles.length,size);
        entry.material.uniforms.rect.value.set(view.rect.x,view.rect.y,view.rect.span);
        entry.color.image.data.set(tile.rgba);entry.depth.image.data.set(depth);
        entry.color.needsUpdate=entry.depth.needsUpdate=true;
        tiles.push({...tile,id:part.id,label:part.label,rect:view.rect,depth});
      }
      const headMeshes=new Set(asset.pixelParts.flatMap(part=>part.meshes));
      asset.meshes.forEach((mesh,i)=>{mesh.visible=visible[i]&&!headMeshes.has(mesh);});
      renderer.setSize(PART_COMPOSITE_SIZE,PART_COMPOSITE_SIZE,false);
      renderer.render(asset.scene,camera);
      this.entries.forEach((entry,index)=>{entry.mesh.visible=index<tiles.length;});
      const autoClear=renderer.autoClear;
      try{renderer.autoClear=false;renderer.render(this.scene,this.camera);}
      finally{renderer.autoClear=autoClear;}
    }finally{asset.meshes.forEach((mesh,i)=>{mesh.visible=visible[i];});}
    return tiles;
  }
  releaseEntry(entry){
    this.scene.remove(entry.mesh);entry.mesh.geometry.dispose();entry.material.dispose();entry.color.dispose();entry.depth.dispose();
  }
  reset(){for(const entry of this.entries)this.releaseEntry(entry);this.entries=[];}
  dispose(){this.reset();this.depthTarget.dispose();this.depthMaterial.dispose();}
}
