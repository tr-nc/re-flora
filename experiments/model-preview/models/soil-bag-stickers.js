import {soilBagStickerArt} from './soil-bag-sticker-art.mjs';

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
  const {viewBox,shapes}=soilBagStickerArt(settings.stickerDesign);
  // Draw continuous curves at the actual texture resolution, with normal Canvas
  // contour antialiasing. Only the shared model pipeline makes the pixel-style tile.
  ctx.save();ctx.scale(w/viewBox.width,h/viewBox.height);
  for(const shape of shapes){
    ctx.fillStyle=shape.fill==='background'?settings.labelColor:settings.inkColor;
    ctx.beginPath();
    if(shape.kind==='path')for(const [method,...args] of shape.commands)ctx[method](...args);
    else if(shape.kind==='ellipse')ctx.ellipse(shape.x,shape.y,shape.rx,shape.ry,shape.rotation,0,Math.PI*2);
    else ctx.rect(shape.x,shape.y,shape.width,shape.height);
    ctx.fill();
  }
  ctx.restore();
}
