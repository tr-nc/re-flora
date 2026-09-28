// Browser-only additions. The native catalog and published assets are untouched.
import {flowerCatalog as nativeCatalog,flowerGeometry as nativeGeometry} from '../../../assets/models/flower-source.mjs';
import {gilleniaGeometry} from './gillenia-geometry.mjs';

const geranium=nativeCatalog.find(spec=>spec.id==='wild-geranium');
export const previewOnlyFlowers=[
  {...geranium,id:'white-geranium',label:'白花老鹳草',latin:'Geranium · white-flowered study',
    defaults:{...geranium.defaults,petalColor:'#f4f2ec',innerColor:'#e6dce5',centerColor:'#d7c88c',leafColor:'#638b59'},
    note:'白色五瓣 · 淡粉瓣根 · 掌状裂叶 · 仅网页配色候选'},
  {id:'gillenia',label:'星草梅',latin:'Gillenia trifoliata',kind:'radial',
    heads:[[-.18,1.30,0,.31,-6],[-.83,.95,.08,.28,-24],[.65,1.0,-.1,.30,20],[1.05,.40,.05,.26,35],[-.65,.15,.14,.25,-22],[.39,0,.1,.24,24],[-1.0,-.35,0,.22,-30]],
    defaults:{...geranium.defaults,petalColor:'#f5f4ef',innerColor:'#e5d8cc',centerColor:'#d6caa0',leafColor:'#648343',stemColor:'#865347',tilt:56},
    note:'五片疏开细长白瓣 · 小花心 · 红褐细枝 · 狭长三出锯齿叶 · 按用户参考图制作（仅网页）'},
];
export const flowerCatalog=[...nativeCatalog,...previewOnlyFlowers];
export function flowerGeometry(id,settings={}){
  const spec=previewOnlyFlowers.find(spec=>spec.id===id);
  if(!spec)return nativeGeometry(id,settings);
  const values={...spec.defaults,...settings};
  return id==='white-geranium'?nativeGeometry('wild-geranium',values):gilleniaGeometry(spec,values);
}
