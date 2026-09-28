// Browser-only additions. The native catalog and published assets are untouched.
import {flowerCatalog as nativeCatalog,flowerGeometry as nativeGeometry} from '../../../assets/models/flower-source.mjs';
import {strawberryGeometry} from './strawberry-geometry.mjs';

const geranium=nativeCatalog.find(spec=>spec.id==='wild-geranium');
export const previewOnlyFlowers=[
  {...geranium,id:'white-geranium',label:'白花老鹳草',latin:'Geranium · white-flowered study',
    defaults:{...geranium.defaults,petalColor:'#f4f2ec',innerColor:'#e6dce5',centerColor:'#d7c88c',leafColor:'#638b59'},
    note:'白色五瓣 · 淡粉瓣根 · 掌状裂叶 · 仅网页配色候选'},
  {id:'star-strawberry',label:'星草莓 · 白色草莓花',latin:'Fragaria · strawberry flower study',kind:'radial',
    heads:[[-.26,.55,0,.46,-8],[.46,.22,-.05,.37,20]],
    defaults:{...geranium.defaults,petalColor:'#f8f5e9',innerColor:'#eee7bf',centerColor:'#e8bd43',leafColor:'#527c48',stemColor:'#64834c',tilt:48},
    note:'五片圆白瓣 · 金黄花心与雄蕊 · 三出锯齿叶 · 仅网页造型候选'},
];
export const flowerCatalog=[...nativeCatalog,...previewOnlyFlowers];
export function flowerGeometry(id,settings={}){
  const spec=previewOnlyFlowers.find(spec=>spec.id===id);
  if(!spec)return nativeGeometry(id,settings);
  const values={...spec.defaults,...settings};
  return id==='white-geranium'?nativeGeometry('wild-geranium',values):strawberryGeometry(spec,values);
}
