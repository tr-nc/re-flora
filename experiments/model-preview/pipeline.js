import * as THREE from 'three';
import {projectGroups} from './geometry.js';
import {repairImage,quantizeImage} from './postprocess.mjs';

// One rendering/processing pipeline for every adapter. Models own appearance,
// not renderers, clocks, image repair, canvas controls or download behavior.
export class PreviewPipeline{
  constructor(sourceCanvas,pixelCanvas){
    this.source=new THREE.WebGLRenderer({canvas:sourceCanvas,alpha:true,antialias:true,preserveDrawingBuffer:true});
    this.pixel=new THREE.WebGLRenderer({canvas:pixelCanvas,alpha:true,antialias:false,preserveDrawingBuffer:true});
    for(const renderer of [this.source,this.pixel]){
      renderer.setClearColor(0,0);renderer.outputColorSpace=THREE.SRGBColorSpace;
      renderer.toneMapping=THREE.NoToneMapping;renderer.shadowMap.type=THREE.PCFShadowMap;
    }
    this.source.setPixelRatio(Math.min(devicePixelRatio||1,2));this.pixel.setPixelRatio(1);
    this.idTarget=new THREE.WebGLRenderTarget(1,1);this.idTarget.texture.colorSpace=THREE.SRGBColorSpace;
    this.colorTarget=new THREE.WebGLRenderTarget(1,1);this.colorTarget.texture.colorSpace=THREE.SRGBColorSpace;
    this.idMaterials=new Map();
    this.screen=new THREE.Scene();this.screenCamera=new THREE.Camera();
    this.screenMaterial=new THREE.ShaderMaterial({
      uniforms:{image:{value:null}},depthTest:false,depthWrite:false,
      vertexShader:'varying vec2 tex; void main(){tex=uv;gl_Position=vec4(position.xy,0.,1.);}',
      // Input bytes already contain the displayed sRGB values, not linear light.
      fragmentShader:'uniform sampler2D image; varying vec2 tex; void main(){gl_FragColor=texture2D(image,tex);}',
    });
    this.screen.add(new THREE.Mesh(new THREE.PlaneGeometry(2,2),this.screenMaterial));
    this.last=null;
  }
  resize(sourceSize,size){
    this.source.setSize(sourceSize,sourceSize,false);
    if(this.size===size)return;
    this.size=size;this.pixel.setSize(size,size,false);this.idTarget.setSize(size,size);
    this.bytes=new Uint8Array(size*size*4);this.idBytes=new Uint8Array(this.bytes.length);
    this.texture?.dispose();this.texture=new THREE.DataTexture(new Uint8Array(this.bytes.length),size,size);
    this.texture.minFilter=this.texture.magFilter=THREE.NearestFilter;
    this.screenMaterial.uniforms.image.value=this.texture;
  }
  render(asset,sourceCamera,pixelCamera,settings){
    const {time,clip,wireframe,levels}=settings;
    asset.sample(time,clip);asset.scene.updateMatrixWorld(true);sourceCamera.updateMatrixWorld(true);pixelCamera.updateMatrixWorld(true);
    this.source.shadowMap.enabled=this.pixel.shadowMap.enabled=asset.shadows;
    asset.preparePass('source');
    const materials=new Set(asset.meshes.flatMap(mesh=>Array.isArray(mesh.material)?mesh.material:[mesh.material]));
    const wires=[...materials].map(material=>[material,material.wireframe]);
    try{
      if(wireframe)for(const [material]of wires)material.wireframe=true;
      this.source.render(asset.scene,sourceCamera);
    }finally{for(const [material,value]of wires)material.wireframe=value;}
    asset.preparePass('pixel');this.pixel.render(asset.scene,pixelCamera);
    const gl=this.pixel.getContext();
    gl.readPixels(0,0,this.size,this.size,gl.RGBA,gl.UNSIGNED_BYTE,this.bytes);
    const original=this.bytes.slice(),owners=this.readOwners(asset,pixelCamera);
    const projected=projectGroups(asset,pixelCamera,this.size);
    const {sampleColor,sampleBase}=this.captureSurfaceColors(asset,pixelCamera,levels>0);
    const result=repairImage(original,owners,projected,this.size,sampleColor);
    const fallback=new Map(projected.map(group=>[group.id,group.fallbackColor]));
    const output=quantizeImage(result.rgba,levels,i=>sampleBase?.(result.owners[i],i,this.size)??fallback.get(result.owners[i]));
    this.texture.image.data.set(output);this.texture.needsUpdate=true;
    this.pixel.render(this.screen,this.screenCamera);
    this.last={sourceTime:time,pixelTime:time,repair:{added:result.added,groups:result.groups},projectedGroups:projected,original,owners};
    return this.last;
  }
  captureSurfaceColors(asset,camera,withPalette=false){
    const resolution=Math.max(256,Math.min(512,this.size*4));
    this.colorTarget.setSize(resolution,resolution);
    this.pixel.setRenderTarget(this.colorTarget);this.pixel.render(asset.scene,camera);
    const colors=new Uint8Array(resolution*resolution*4);
    this.pixel.readRenderTargetPixels(this.colorTarget,0,0,resolution,resolution,colors);
    this.pixel.setRenderTarget(null);
    const owners=this.readOwners(asset,camera,resolution);
    let bases;
    if(withPalette&&asset.palettePass){
      asset.preparePass('palette');
      try{
        this.pixel.setRenderTarget(this.colorTarget);this.pixel.render(asset.scene,camera);
        bases=new Uint8Array(colors.length);
        this.pixel.readRenderTargetPixels(this.colorTarget,0,0,resolution,resolution,bases);
      }finally{this.pixel.setRenderTarget(null);asset.preparePass('pixel');}
    }
    const nearestSample=(id,pixel,size)=>{
      const x=pixel%size,y=Math.floor(pixel/size),startX=Math.floor(x*resolution/size),endX=Math.ceil((x+1)*resolution/size);
      const startY=Math.floor(y*resolution/size),endY=Math.ceil((y+1)*resolution/size);
      let nearest=-1,distance=Infinity;
      for(let sy=startY;sy<endY;sy++)for(let sx=startX;sx<endX;sx++){
        const i=sy*resolution+sx;
        if(owners[i]!==id||!colors[i*4+3])continue;
        const d=(sx+.5-(x+.5)*resolution/size)**2+(sy+.5-(y+.5)*resolution/size)**2;
        if(d<distance){distance=d;nearest=i;}
      }
      return nearest;
    };
    return {
      sampleColor:(id,pixel,size)=>{
        const nearest=nearestSample(id,pixel,size);
        return nearest<0?null:Array.from(colors.subarray(nearest*4,nearest*4+3));
      },
      sampleBase:bases?(id,pixel,size)=>{
        const nearest=nearestSample(id,pixel,size);
        return nearest<0?null:Array.from(bases.subarray(nearest*4,nearest*4+3));
      }:null,
    };
  }
  readOwners(asset,camera,size=this.size){
    this.idTarget.setSize(size,size);
    const bytes=size===this.size?this.idBytes:new Uint8Array(size*size*4);
    const saved=asset.meshes.map(mesh=>[mesh,mesh.material]);
    const groupFor=new Map(asset.repairGroups.flatMap(group=>group.meshes.map(mesh=>[mesh,group.id])));
    try{
      for(const [mesh,original]of saved){
        const id=groupFor.get(mesh)??0;
        const side=(Array.isArray(original)?original[0]:original).side;
        const key=`${id}:${side}`;
        if(!this.idMaterials.has(key))this.idMaterials.set(key,new THREE.MeshBasicMaterial({color:id,side,toneMapped:false}));
        mesh.material=this.idMaterials.get(key);
      }
      this.pixel.shadowMap.enabled=false;this.pixel.setRenderTarget(this.idTarget);
      this.pixel.render(asset.scene,camera);
      this.pixel.readRenderTargetPixels(this.idTarget,0,0,size,size,bytes);
    }finally{
      for(const [mesh,material]of saved)mesh.material=material;
      this.pixel.setRenderTarget(null);this.pixel.shadowMap.enabled=asset.shadows;
    }
    return Uint32Array.from({length:size*size},(_,i)=>
      (bytes[i*4]<<16)|(bytes[i*4+1]<<8)|bytes[i*4+2]);
  }
  releaseAsset(){
    for(const material of this.idMaterials.values())material.dispose();this.idMaterials.clear();
    this.source.renderLists.dispose();this.pixel.renderLists.dispose();this.last=null;
  }
  dispose(){
    this.releaseAsset();this.idTarget.dispose();this.colorTarget.dispose();this.texture?.dispose();
    this.screen.children[0].geometry.dispose();this.screenMaterial.dispose();
    this.source.dispose();this.pixel.dispose();
  }
}
