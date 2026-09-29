// Opaque RGB weight maps: RGB = A/B/C; D = max(0, 1-R-G-B).
// No color-space decoding on the weights. Palette colors mix in linear light.
export const maskPresets=[['root-tip','瓣根 → 瓣尖'],['blue-white','瓣中 → 白边'],['veins','细脉贴纸'],['solid','纯色分区']];
export const defaultPalette=['#6099df','#f6f4ed','#eac457','#5d834e'];
const clamp=v=>Math.max(0,Math.min(1,v));
export const srgbToLinear=v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4;
export const linearToSrgb=v=>v<=.0031308?v*12.92:1.055*Math.max(0,v)**(1/2.4)-.055;
export function weightsFromRGB(rgb){
  const weights=rgb.map(v=>clamp(v/255));weights.push(Math.max(0,1-weights.reduce((a,b)=>a+b,0)));
  const sum=weights.reduce((a,b)=>a+b,0);return weights.map(v=>v/sum);
}
export function validateMask(mask){
  if(!mask||!Number.isInteger(mask.width)||!Number.isInteger(mask.height)||mask.width<32||mask.height<32||mask.width>512||mask.height>512||mask.pixels?.length!==mask.width*mask.height*4)throw new Error('权重图须为 32–512 像素宽高的 RGBA 图像');
  for(let i=0;i<mask.pixels.length;i++)if(!Number.isInteger(mask.pixels[i])||mask.pixels[i]<0||mask.pixels[i]>255||(i%4===3&&mask.pixels[i]!==255))throw new Error('权重图必须不透明，通道值须为 0–255；alpha 不作为权重');
  return mask;
}
export function createMask(preset='root-tip',size=128){
  if(!maskPresets.some(([id])=>id===preset))throw new Error(`未知权重模板：${preset}`);
  if(!Number.isInteger(size)||size<32||size>512)throw new Error('权重图大小须为 32–512');
  const pixels=new Uint8Array(size*size*4);
  for(let y=0;y<size;y++)for(let x=0;x<size;x++){
    const u=(x+.5)/size,v=(y+.5)/size;let rgb;
    if(u>=.75)rgb=v<.5?[0,0,255]:[0,0,0];
    else{
      const w=Math.abs(u/.75*2-1);
      let b=preset==='solid'?0:preset==='blue-white'?clamp((w-.2)/.55):clamp((v-.12)/.70);
      if(preset==='veins')b=clamp(.4+.6*v-.5*Math.exp(-(((u/.75-.5)*20)**2))-.18*Math.cos((u/.75-.5)*35+v*10));
      b=b*b*(3-2*b);rgb=[Math.round(255*(1-b)),Math.round(255*b),0];
    }
    pixels.set([...rgb,255],(y*size+x)*4);
  }
  return {width:size,height:size,pixels};
}
export function maskValue(value){return typeof value==='string'?createMask(value):validateMask(value);}
export function resolvePalette(mask,palette=defaultPalette){
  validateMask(mask);
  if(palette.length!==4||palette.some(c=>!/^#[\da-f]{6}$/i.test(c)))throw new Error('需要四个 #RRGGBB 调色板颜色');
  const colors=palette.map(c=>[1,3,5].map(i=>srgbToLinear(parseInt(c.slice(i,i+2),16)/255)));
  const pixels=new Uint8Array(mask.pixels.length);
  for(let i=0;i<pixels.length;i+=4){
    const weights=weightsFromRGB(Array.from(mask.pixels.slice(i,i+3)));
    for(let c=0;c<3;c++)pixels[i+c]=Math.round(clamp(linearToSrgb(weights.reduce((sum,w,k)=>sum+w*colors[k][c],0)))*255);
    pixels[i+3]=255;
  }
  return pixels;
}
// Pure paint operation: callers own history/undo. Soft edges blend weights, not
// palette indices or encoded colors. A new map is returned; defaults stay immutable.
export function paintMask(mask,x,y,slot,radius,strength=1){
  validateMask(mask);
  if(!Number.isInteger(slot)||slot<0||slot>3||![x,y,radius,strength].every(Number.isFinite)||radius<=0)throw new Error('无效画笔');
  const pixels=new Uint8Array(mask.pixels),target=[0,0,0];if(slot<3)target[slot]=255;
  for(let yy=Math.max(0,Math.floor(y-radius));yy<Math.min(mask.height,Math.ceil(y+radius));yy++)for(let xx=Math.max(0,Math.floor(x-radius));xx<Math.min(mask.width,Math.ceil(x+radius));xx++){
    const distance=Math.hypot(xx+.5-x,yy+.5-y)/radius;
    if(distance>=1)continue;
    const a=clamp(strength)*(1-distance*distance),i=(yy*mask.width+xx)*4;
    for(let c=0;c<3;c++)pixels[i+c]=Math.round(pixels[i+c]*(1-a)+target[c]*a);
  }
  return {width:mask.width,height:mask.height,pixels};
}
