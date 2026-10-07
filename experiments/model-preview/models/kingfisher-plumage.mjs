// Original deterministic opaque plumage, never a photograph pasted onto the
// model. Feather direction and overlapping color transitions are authored in
// the same UV domains as the 3D skin. Palette resolution stays in linear light.
import {srgbToLinear,linearToSrgb} from '../../../assets/models/palette-mask.mjs';
import {kingfisherProfiles} from './kingfisher-geometry.mjs';
export const kingfisherColorKeys=['bodyColor','backColor','bellyColor','wingColor','accentColor','throatColor','cheekColor'];
const TAU=Math.PI*2,clamp=x=>Math.max(0,Math.min(1,x)),smooth=(a,b,x)=>{const t=clamp((x-a)/(b-a));return t*t*(3-2*t);};
const hash=n=>{const x=Math.sin(n*127.1+17.3)*43758.5453;return x-Math.floor(x);};
function weights(slot){const w=Array(7).fill(0);w[kingfisherColorKeys.indexOf(slot)]=1;return w;}
function mix(w,slot,amount){const a=clamp(amount),k=kingfisherColorKeys.indexOf(slot);for(let i=0;i<w.length;i++)w[i]*=1-a;w[k]+=a;}
function ellipse(u,v,x,y,rx,ry){return ((u-x)/rx)**2+((v-y)/ry)**2;}
// Tiny directional strokes, not uniform noise or chunky isolated polygons.
function featherGrain(u,v,frequency=38){
  const row=Math.floor(v*frequency),phase=u*frequency*1.7+hash(row)*.6;
  const strand=.5+.5*Math.sin(phase*TAU+v*frequency*.9);
  return .83+.12*strand+.05*(.5+.5*Math.sin(u*931+v*719));
}
function headSample(u,v){
  const y=kingfisherProfiles.head[0][0]+v*(kingfisherProfiles.head.at(-1)[0]-kingfisherProfiles.head[0][0]);
  const a=u*TAU,front=Math.sin(a),side=Math.abs(Math.cos(a)),w=weights('cheekColor');
  const upper=smooth(.77,.94,y);mix(w,'accentColor',upper*.70);
  // Side crown patches with copper-red feather tips above them. An entire
  // solid pink latitude cap reads as a helmet even on a correctly sized head.
  for(const cx of [1.012,.488]){
    const uu=u<.20?u+1:u,d=ellipse(uu+.006*Math.sin(y*213),y+.007*Math.sin(u*371),cx,.94,.14,.15);
    const strand=.78+.22*Math.sin(u*817+y*293);
    const patch=(1-smooth(.42,1.20,d))*smooth(.55,.88,side);
    mix(w,'bodyColor',patch*strand);
    mix(w,'accentColor',patch*smooth(.67,.96,.5+.5*Math.sin(u*931+y*219))*.17);
  }
  const tips=smooth(1.035,1.15,y);mix(w,'accentColor',tips*.86);mix(w,'backColor',tips*.32);
  for(const cx of [.94,.56]){
    const uu=u<.12?u+1:u;
    const ear=ellipse(uu+.005*Math.sin(y*139),y,cx,.735,.052,.052);
    mix(w,'wingColor',(1-smooth(.65,1.18,ear))*smooth(.66,.85,side));
    const white=ellipse(uu,y,cx-.018,.596,.069,.039);
    mix(w,'throatColor',1-smooth(.58,1.24,white));
  }
  const throat=smooth(.82,.95,front)*(1-smooth(.64,.73,y));
  mix(w,'throatColor',throat);
  mix(w,'accentColor',(1-smooth(.69,.80,y))*smooth(.45,.9,front)*.12);
  const barb=.5+.5*Math.sin(u*TAU*119+v*53+.7*Math.sin(v*TAU*17));
  return {w,shade:(.66+.32*barb)*(1-.22*tips)};
}
function bodySample(u,v){
  const y=kingfisherProfiles.body[0][0]+v*(kingfisherProfiles.body.at(-1)[0]-kingfisherProfiles.body[0][0]);
  const front=Math.sin(u*TAU),w=weights('bellyColor');
  // Lemon breast with a pale lower fringe and a warm shoulder, not ochre.
  mix(w,'throatColor',(1-smooth(-.86,-.34,y))*.18+smooth(.4,.58,front)*.04);
  mix(w,'cheekColor',smooth(.16,.55,y)*.56);
  mix(w,'accentColor',smooth(.32,.60,y)*.35);
  const back=1-smooth(-.55,-.12,front);
  mix(w,'backColor',back*.85);
  mix(w,'wingColor',back*(1-smooth(-.90,-.30,y))*.13);
  // Narrow orange-rufous rear mantle, visible around the folded wing.
  const stripe=(1-smooth(-.90,-.63,front))*smooth(-.9,-.7,y);
  mix(w,'cheekColor',stripe*.73);mix(w,'accentColor',stripe*.38);
  mix(w,'bodyColor',(1-smooth(-.93,-.83,y))*(1-smooth(-1,-.90,front))*.18);
  mix(w,'bellyColor',(1-smooth(-.77,-.54,y))*.84);
  mix(w,'throatColor',(1-smooth(-.95,-.81,y))*.12);
  const throat=smooth(.86,.99,front)*smooth(.42,.61,y);mix(w,'throatColor',throat);
  return {w,shade:featherGrain(u+.07*v,v,47)};
}
function wingSample(u,v){
  const w=weights('backColor'),shaft=.5+.5*Math.cos((u+.08*v)*TAU*10);
  let shade=.68+.34*shaft;
  mix(w,'wingColor',smooth(.57,.84,v)*shaft**15*.12);
  mix(w,'throatColor',(.5+.5*Math.sin(u*57-v*21))*.016);
  // Photo-positioned irregular highlights, concentrated along two curved
  // covert rows. Short soft flecks replace the previous long blue seeds/grid.
  const flecks=[[.60,.13],[.66,.23],[.70,.32],[.70,.42],[.68,.54],[.66,.65],[.63,.73],
    [.45,.22],[.49,.33],[.47,.44],[.43,.55],[.46,.64],[.44,.74],
    [.32,.31],[.34,.44],[.31,.55],[.29,.66],[.56,.48]];
  for(let i=0;i<flecks.length;i++){
    const [cu,cv]=flecks[i],rx=.029+hash(i+9)*.014,ry=.025+hash(i+18)*.033;
    const d=ellipse(u+.017*Math.sin(v*13),v,cu,cv,rx,ry);if(d>1.65)continue;
    const edge=1-smooth(.35,1.55,d),tip=smooth(cv-ry*.6,cv+ry*.45,v);
    const fleckGrain=.72+.28*(.5+.5*Math.sin(u*731+v*413));
    mix(w,'wingColor',edge*tip*(.48+.44*hash(i+71))*fleckGrain);
    mix(w,'throatColor',edge*(1-tip)*.036);
  }
  const fringe=smooth(.66+.16*Math.sin(v*Math.PI)-.026,.66+.16*Math.sin(v*Math.PI)+.017,u);
  mix(w,'throatColor',fringe*(.06+.05*Math.sin(v*571)**2));
  mix(w,'bellyColor',fringe*.10);
  const barb=.5+.5*Math.sin((u*171+v*39)*TAU);
  shade*=.81+.19*barb;
  return {w,shade};
}
function tailSample(u,v){const w=weights('bodyColor');mix(w,'accentColor',.48+smooth(.22,.92,v)*.43);mix(w,'backColor',smooth(.65,1,v)*.20);return {w,shade:featherGrain(u,v,28)};}
function billSample(u,v){const w=weights('accentColor'),ridge=Math.max(1-smooth(0,.19,u),smooth(.81,1,u));mix(w,'cheekColor',ridge*.34);mix(w,'throatColor',ridge*.035);return {w,shade:.78+.22*ridge+.035*v};}
const samples={head:headSample,body:bodySample,wing:wingSample,tail:tailSample,bill:billSample};
export function createKingfisherPlumage(size=256){
  if(!Number.isInteger(size)||size<32||size>512)throw new Error('Plumage size must be 32–512');
  return Object.fromEntries(Object.entries(samples).map(([key,sample])=>{
    const weights=new Float32Array(size*size*7),shades=new Float32Array(size*size);
    for(let y=0;y<size;y++)for(let x=0;x<size;x++){const i=y*size+x,{w,shade}=sample((x+.5)/size,(y+.5)/size);weights.set(w,i*7);shades[i]=shade;}
    return [key,{size,weights,shades}];
  }));
}
export function resolveKingfisherPlumage(mask,palette,pixels=new Uint8Array(mask.size*mask.size*4)){
  const colors=kingfisherColorKeys.map(key=>{
    if(!/^#[\da-f]{6}$/i.test(palette[key]))throw new Error(`Invalid kingfisher palette: ${key}`);
    return [1,3,5].map(i=>srgbToLinear(parseInt(palette[key].slice(i,i+2),16)/255));
  });
  if(pixels.length!==mask.size*mask.size*4)throw new Error('Invalid plumage pixel buffer');
  for(let i=0;i<mask.shades.length;i++){
    for(let c=0;c<3;c++){
      let linear=0;for(let k=0;k<7;k++)linear+=mask.weights[i*7+k]*colors[k][c];
      pixels[i*4+c]=Math.round(clamp(linearToSrgb(linear*mask.shades[i]))*255);
    }pixels[i*4+3]=255;
  }return pixels;
}
