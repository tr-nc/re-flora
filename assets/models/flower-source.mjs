// The one shared head generator for web and native publication. Species are
// presets, never independent geometry branches. Game owns stems/placement.
import {parametricFlower,flowerShapeDefaults} from './parametric-flower.mjs';
export const flowerShapes={
  // Five broad spoons form an open bowl; separated shoulders and rolled lips.
  'wild-geranium':{petalCount:5,petalWidth:.34,petalShoulder:.60,tipSharpness:0,opening:.64,petalCup:.23,petalArch:.04,tipCurl:-.26,poseVariation:.085,centerHeight:.17},
  // Compact rounded crown: a raised heart over short, softly hanging petals.
  'forget-me-not':{flowerSize:.90,petalCount:5,petalWidth:.34,petalLength:.70,petalShoulder:.62,opening:0,centerRadius:.20,centerHeight:.32,petalCup:.09,petalArch:.02,tipCurl:-.16,poseVariation:.045},
  // Strap-like rays arch over a low cushion, rather than a flat pinwheel.
  'oxeye-daisy':{petalCount:14,petalWidth:.115,petalShoulder:.62,tipSharpness:.05,opening:.10,petalCup:.045,petalArch:.20,tipCurl:-.17,poseVariation:.055,centerRadius:.27,centerHeight:.17},
  // Broad notched fans flare from a shallow bowl with an uneven rolled rim.
  cosmos:{petalCount:8,petalWidth:.28,petalShoulder:.64,tipSharpness:.05,notch:.20,opening:.34,petalCup:.13,petalArch:.07,tipCurl:-.23,poseVariation:.10,centerRadius:.21,centerHeight:.18},
  // A tall cone above long, ridged, individually drooping rays.
  coneflower:{petalCount:12,petalWidth:.14,petalShoulder:.38,petalLength:1.1,tipSharpness:.4,opening:-.57,petalCup:.065,petalArch:.11,tipCurl:-.13,poseVariation:.085,centerRadius:.30,centerHeight:.55,centerShape:'cone'},
  // Upright overlapping spoons wrap around the cup; tips open slightly out.
  tulip:{petalCount:6,petalWidth:.49,petalShoulder:.67,petalLength:1.08,tipSharpness:0,opening:.98,petalCup:.14,petalArch:.06,tipCurl:-.08,poseVariation:.04,centerRadius:.15,centerHeight:.06},
  sunflower:{flowerSize:1.3,petalCount:20,petalWidth:.16,petalLength:.85,petalShoulder:.56,tipSharpness:.65,opening:.08,petalCup:.045,petalArch:.1,tipCurl:-.10,poseVariation:.035,centerRadius:.35,centerHeight:.19},
};
const entry=(id,displayName,stemLayers,label,latin,tilt,palette)=>({id,displayName,stemLayers,label,latin,
  cacheFamily:id,defaults:{...flowerShapeDefaults,...flowerShapes[id],tilt,
    paletteA:palette[0],paletteB:palette[1],paletteC:palette[2],paletteD:palette[3],weightMap:'root-tip'}});
// Stable IDs/order preserve old gardens; layers are species-owned art tuning.
// Species silhouettes share the
// same surface mechanism; no per-species mesh or rendering branches.
export const flowerCatalog=[
  entry('wild-geranium','Wild Geranium',41,'五瓣野花 · 野老鹳草','Geranium maculatum',38,['#be78a6','#d695c5','#ecd18a','#52734a']),
  entry('forget-me-not','Forget-me-not',42,'勿忘草','Myosotis sylvatica',28,['#e5ecda','#83b9eb','#e7ba48','#52734a']),
  entry('oxeye-daisy','Oxeye Daisy',42,'白色滨菊','Leucanthemum vulgare',42,['#e5d9a9','#f4f0da','#e4b53f','#52734a']),
  entry('cosmos','Cosmos',42,'波斯菊','Cosmos bipinnatus',44,['#b94880','#e4a0c0','#e9be4c','#52734a']),
  entry('coneflower','Coneflower',43,'紫松果菊','Echinacea purpurea',55,['#b36c98','#d18bb4','#b77c3c','#52734a']),
  entry('tulip','Tulip',37,'郁金香','Tulipa',70,['#f3c38e','#e8a075','#755048','#52734a']),
  entry('sunflower','Sunflower',136,'向日葵','Helianthus annuus',70,['#d99922','#ffdc52','#664323','#4f7135']),
];
export function flowerGeometry(id,settings={}){
  const spec=flowerCatalog.find(spec=>spec.id===id);
  if(!spec)throw new Error(`Unknown flower: ${id}`);
  return parametricFlower({...spec.defaults,...settings});
}
