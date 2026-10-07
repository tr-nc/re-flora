import * as THREE from 'three';
import {soilBagClips,soilBagDefaults,soilBagPresets,soilBagGeometry,soilBagPose,soilBagFlowGeometry} from './soil-bag-geometry.mjs';
import {drawSoilBagSticker} from './soil-bag-stickers.js';
import {disposeScene} from './resources.js';

function geometryFor(part){
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));
  geometry.setAttribute('uv',new THREE.Float32BufferAttribute(part.uvs,2));
  geometry.setIndex(part.indices);geometry.computeVertexNormals();return geometry;
}
function updateGeometry(mesh,part){
  mesh.geometry.attributes.position.array.set(part.positions);
  mesh.geometry.attributes.position.needsUpdate=true;
  mesh.geometry.computeVertexNormals();mesh.geometry.computeBoundingSphere();
}
function paperTexture(){
  const canvas=document.createElement('canvas');canvas.width=canvas.height=64;
  const ctx=canvas.getContext('2d'),data=ctx.createImageData(64,64);
  for(let i=0;i<64*64;i++){
    const value=240+(i*31%16);data.data.set([value,value,value,255],i*4);
  }
  ctx.putImageData(data,0,0);
  const texture=new THREE.CanvasTexture(canvas);texture.colorSpace=THREE.SRGBColorSpace;
  texture.wrapS=texture.wrapT=THREE.RepeatWrapping;texture.repeat.set(3,3);return texture;
}
export const soilBagDefinition={
  id:'soil-bag',label:'营养土袋 · 网页候选',defaults:soilBagDefaults,
  preview:{resolution:64,background:'#2b3831'},
  presetLabel:'包装预设 · 可以继续改色与换贴纸',presets:soilBagPresets,
  controls:[
    {type:'note',label:'先验收网页外观和动作，不改变游戏里的Soil工具。撕开片段只生成一个角部袋口；倾倒片段从已开口的袋子开始。'},
    {key:'bagColor',label:'袋子颜色',type:'color'},
    {key:'fullness',label:'袋子充盈 / 鼓起程度',min:.35,max:1.25,step:.01},
    {key:'sideFlatness',label:'侧面扁平度（上下更饱满）',min:0,max:1,step:.01},
    {type:'note',label:'0 = 原来的圆弧侧面；1 = 上下更饱满、中段更平直。充盈度调厚度，扁平度调侧面轮廓；点“侧面”最容易看区别。'},
    {key:'stickerDesign',label:'更换正面贴纸',type:'select',options:[['sprout','双叶芽 · 大图案'],['layers','三层土 · 大图案'],['flower','五瓣花 · 大图案'],['custom','自定义贴纸（先导入图片）']]},
    {key:'labelColor',label:'贴纸底色',type:'color'},
    {key:'inkColor',label:'内置贴纸图案 / 色块颜色',type:'color'},
    {key:'labelScale',label:'贴纸大小',min:.7,max:1.08,step:.01},
    {key:'customSticker',label:'导入自定义贴纸 · PNG / JPG / WebP',type:'image',accept:'image/png,image/jpeg,image/webp'},
    {type:'note',label:'贴纸使用 Nearest 最近邻采样，不混糊边缘。内置贴纸只有大图案与实心色块，没有小字；导入保持比例，透明部分铺在底色上。调整只留在本次页面，不自动发布游戏资产。'},
    {type:'note',label:'时间轴可选“撕开”和“倾倒”，播放一次后停在末帧，拖动或逐帧检查。土流仅示意重力方向，不含土块粒子细节。'},
  ],
  async create(){
    const scene=new THREE.Scene(),root=new THREE.Group(),tab=new THREE.Group();scene.add(root);root.add(tab);
    const canvas=document.createElement('canvas');canvas.width=512;canvas.height=640;
    const stickerTexture=new THREE.CanvasTexture(canvas);stickerTexture.colorSpace=THREE.SRGBColorSpace;
    stickerTexture.minFilter=stickerTexture.magFilter=THREE.NearestFilter;stickerTexture.generateMipmaps=false;
    const paper=paperTexture();
    const materials={
      bag:new THREE.MeshStandardMaterial({map:paper,roughness:.92,side:THREE.DoubleSide}),
      seam:new THREE.MeshStandardMaterial({roughness:.95,side:THREE.DoubleSide}),
      lining:new THREE.MeshStandardMaterial({roughness:1,side:THREE.DoubleSide}),
      sticker:new THREE.MeshStandardMaterial({map:stickerTexture,roughness:.91,side:THREE.DoubleSide}),
      soil:new THREE.MeshStandardMaterial({color:'#614331',roughness:1,side:THREE.DoubleSide,flatShading:true}),
    };
    const recipe=soilBagGeometry(),meshes=recipe.parts.map(part=>{
      const mesh=new THREE.Mesh(geometryFor(part),materials[part.material]);mesh.name=part.name;
      (part.group==='tab'?tab:root).add(mesh);return mesh;
    });
    tab.position.fromArray(recipe.tabHinge);
    const flowMesh=new THREE.Mesh(geometryFor(soilBagFlowGeometry(soilBagPose(0))),materials.soil);flowMesh.name='Simplified gravity flow';flowMesh.visible=false;scene.add(flowMesh);meshes.push(flowMesh);
    const repairGroups=[{id:1,label:'袋体 / 单一袋口 / 贴纸',meshes:meshes.filter(mesh=>mesh!==flowMesh)},{id:2,label:'简化土流（非粒子）',meshes:[flowMesh],fallbackColor:[97,67,49]}];
    const key=new THREE.DirectionalLight('#fff3de',2.05);key.position.set(-3,4,6);scene.add(key,new THREE.AmbientLight('#e5f0e9',.85));
    let settings={...soilBagDefaults},pose=soilBagPose(0),currentClip=0,customImage=null,disposed=false,imageRequest=0,labelKey='';
    const drawLabel=()=>{
      const nextKey=JSON.stringify([settings.stickerDesign,settings.labelColor,settings.inkColor,imageRequest]);
      if(nextKey===labelKey)return;
      drawSoilBagSticker(canvas,settings,customImage);stickerTexture.needsUpdate=true;labelKey=nextKey;
    };
    return {
      scene,meshes,repairGroups,clips:soilBagClips,
      view:{span:3.25,target:[.03,-.13,0],offset:[1.9,1.05,6.5],near:.1,far:40},
      description:'营养土袋网页候选 · 可换袋色与贴纸 · 撕开 / 倾倒 · 复用苹果与小花的共享像素处理',
      apply(values){
        settings={...settings,...values};materials.bag.color.set(settings.bagColor);
        materials.seam.color.set(settings.bagColor).multiplyScalar(.76);
        materials.lining.color.set(settings.labelColor).multiplyScalar(.43);
        repairGroups[0].fallbackColor=[1,3,5].map(i=>parseInt(settings.bagColor.slice(i,i+2),16));
        drawLabel();
      },
      async importImage(key,file){
        if(key!=='customSticker')throw new Error('未知的贴纸输入');
        if(file.size>8*1024*1024)throw new Error('贴纸文件请小于8 MB');
        const token=++imageRequest,image=await createImageBitmap(file);
        if(disposed||token!==imageRequest){image.close();return null;}
        if(image.width>8192||image.height>8192){image.close();throw new Error('贴纸边长请不超过8192像素');}
        customImage?.close();customImage=image;labelKey='';
        return {stickerDesign:'custom'};
      },
      sample(time,clip){
        currentClip=clip;pose=soilBagPose(time,clip);
        const next=soilBagGeometry({opening:pose.opening,fullness:settings.fullness*(.75+.25*pose.fill),sideFlatness:settings.sideFlatness,labelScale:settings.labelScale,fill:pose.fill});
        next.parts.forEach((part,i)=>updateGeometry(meshes[i],part));
        root.position.fromArray(pose.root);root.rotation.set(0,0,pose.angle);tab.rotation.z=pose.tabAngle;
        flowMesh.visible=pose.flow>1e-5;
        if(flowMesh.visible)updateGeometry(flowMesh,soilBagFlowGeometry(pose,time));
      },
      get stateDescription(){
        if(settings.stickerDesign==='custom'&&!customImage)return '尚未导入贴纸 · 暂用嫩芽图案';
        if(currentClip===0)return pose.opening<.001?'完整封口':pose.opening>.999?'已撕开 · 单一袋口':`撕开袋口 · ${Math.round(pose.opening*100)}%`;
        return pose.flow>1e-5?'袋口朝下 · 简化土流':pose.phase>.99?'倾倒结束 · 保留开口':'已开口 · 准备 / 收回倾倒';
      },
      reviewState(){return {opening:pose.opening,openingCount:pose.opening>0?1:0,mouthNormal:[...pose.mouthNormal],mouthWorld:[...pose.mouthWorld],flowVisible:flowMesh.visible,flow:pose.flow,fill:pose.fill,customSticker:Boolean(customImage),stickerSampling:{minFilter:stickerTexture.minFilter,magFilter:stickerTexture.magFilter,mipmaps:stickerTexture.generateMipmaps,imageSmoothing:canvas.getContext('2d').imageSmoothingEnabled}};},
      preparePass(){},get shadows(){return false;},
      dispose(){disposed=true;imageRequest++;customImage?.close();customImage=null;disposeScene(scene);},
    };
  },
};
