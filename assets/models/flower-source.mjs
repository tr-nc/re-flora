// The one shared head generator for web and native publication. Species are
// presets, never independent geometry branches. Game owns stems/placement.
import {parametricFlower,flowerShapeDefaults} from './parametric-flower.mjs';
export const flowerShapes={
  'wild-geranium':{petalCount:5,petalWidth:.43,tipSharpness:0,opening:.10},
  'forget-me-not':{petalCount:5,petalWidth:.49,petalLength:.75,opening:.06,centerRadius:.13},
  'oxeye-daisy':{petalCount:16,petalWidth:.12,tipSharpness:.2,opening:.02,centerRadius:.25,centerShape:'flat'},
  cosmos:{petalCount:8,petalWidth:.35,tipSharpness:.05,notch:.13,opening:.12,centerRadius:.21},
  coneflower:{petalCount:12,petalWidth:.16,tipSharpness:.45,opening:-.65,centerRadius:.29,centerHeight:.52,centerShape:'cone'},
  tulip:{petalCount:6,petalWidth:.56,tipSharpness:0,opening:.95,centerRadius:.13,centerHeight:.04},
};
const entry=(id,displayName,stemLayers,label,latin,tilt,palette)=>({id,displayName,stemLayers,label,latin,
  cacheFamily:id,defaults:{...flowerShapeDefaults,...flowerShapes[id],tilt,
    paletteA:palette[0],paletteB:palette[1],paletteC:palette[2],paletteD:palette[3],weightMap:'root-tip'}});
// Stable IDs/order/layers preserve old gardens; heads intentionally adopt the
// same approved demo approximations. Forget-me-not is five-petal, not cosmos.
export const flowerCatalog=[
  entry('wild-geranium','Wild Geranium',41,'五瓣野花 · 野老鹳草','Geranium maculatum',38,['#be78a6','#d695c5','#ecd18a','#52734a']),
  entry('forget-me-not','Forget-me-not',42,'勿忘草','Myosotis sylvatica',28,['#e5ecda','#83b9eb','#e7ba48','#52734a']),
  entry('oxeye-daisy','Oxeye Daisy',42,'白色滨菊','Leucanthemum vulgare',42,['#e5d9a9','#f4f0da','#e4b53f','#52734a']),
  entry('cosmos','Cosmos',42,'波斯菊','Cosmos bipinnatus',44,['#b94880','#e4a0c0','#e9be4c','#52734a']),
  entry('coneflower','Coneflower',43,'紫松果菊','Echinacea purpurea',55,['#b36c98','#d18bb4','#b77c3c','#52734a']),
  entry('tulip','Tulip',37,'郁金香','Tulipa',70,['#f3c38e','#e8a075','#755048','#52734a']),
];
export function flowerGeometry(id,settings={}){
  const spec=flowerCatalog.find(spec=>spec.id===id);
  if(!spec)throw new Error(`Unknown flower: ${id}`);
  return parametricFlower({...spec.defaults,...settings});
}
