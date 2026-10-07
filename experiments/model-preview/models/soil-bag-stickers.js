import {soilBagStickerGrid,soilBagStickerMask} from './soil-bag-sticker-art.mjs';

// Opaque, replaceable label art. Alpha in imported images is flattened over labelColor;
// no model-specific alpha/cutout exception is added to the shared pixel pipeline.
export function drawSoilBagSticker(canvas,settings,image=null){
  const ctx=canvas.getContext('2d'),w=canvas.width,h=canvas.height;
  ctx.imageSmoothingEnabled=false;
  ctx.clearRect(0,0,w,h);ctx.fillStyle=settings.labelColor;ctx.fillRect(0,0,w,h);
  if(settings.stickerDesign==='custom'&&image){
    const scale=Math.min((w-36)/image.width,(h-36)/image.height);
    const width=image.width*scale,height=image.height*scale;
    ctx.drawImage(image,(w-width)/2,(h-height)/2,width,height);return;
  }
  const {width,height}=soilBagStickerGrid,mask=soilBagStickerMask(settings.stickerDesign);
  ctx.fillStyle=settings.inkColor;
  // Merge horizontal runs, on integer canvas edges: exactly two colors, no font
  // dependency or antialiased contours before the shared lighting/pixel pipeline.
  for(let y=0;y<height;y++)for(let x=0;x<width;){
    if(!mask[y*width+x]){x++;continue;}
    const start=x;while(x<width&&mask[y*width+x])x++;
    const left=Math.round(start*w/width),top=Math.round(y*h/height);
    ctx.fillRect(left,top,Math.round(x*w/width)-left,Math.round((y+1)*h/height)-top);
  }
}
