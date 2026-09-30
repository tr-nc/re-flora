// Published species use exactly the game presets. Only extra demo studies live here.
import {flowerCatalog as nativeCatalog,flowerShapes as shapes} from '../../../assets/models/flower-source.mjs';
import {parametricFlower,flowerShapeDefaults} from '../../../assets/models/parametric-flower.mjs';
const entry=(id,label,latin,shape,palette,mask='root-tip')=>({id,label,latin,defaults:{...flowerShapeDefaults,...shape,paletteA:palette[0],paletteB:palette[1],paletteC:palette[2],paletteD:palette[3],weightMap:mask}});
const migrated=nativeCatalog;
export const previewOnlyFlowers=[
  entry('white-geranium','白花老鹳草','Geranium · white-flowered study',shapes['wild-geranium'],['#e5d6e2','#faf8f0','#d7c88c','#638b59']),
  entry('gillenia','星草梅','Gillenia trifoliata',{petalCount:5,petalWidth:.12,petalLength:1.15,tipSharpness:.9,centerRadius:.055,centerHeight:.035,tilt:56},['#e5d8cc','#f5f4ef','#d6caa0','#865347']),
  entry('four-petal','四瓣蓝白花 · 可调','Four-petal color study',{petalCount:4,petalWidth:.55,opening:.1,centerShape:'flat'},['#488cdf','#fbfcff','#f4ce67','#5c864d'],'blue-white'),
  entry('five-star','五瓣尖星花 · 可调','Five-point petal study',{petalCount:5,petalWidth:.29,tipSharpness:1,opening:-.16,centerRadius:.14},['#8659c1','#ede0ff','#f6cf71','#648448'],'veins'),
  entry('custom-flower','自定义花朵','Parametric flower studio',{},['#6099df','#f6f4ed','#eac457','#5d834e'],'blue-white'),
];
export const flowerCatalog=[...migrated,...previewOnlyFlowers];
export function flowerGeometry(id,settings={}){
  const spec=flowerCatalog.find(spec=>spec.id===id);if(!spec)throw new Error(`Unknown flower: ${id}`);
  return parametricFlower({...spec.defaults,...settings});
}
