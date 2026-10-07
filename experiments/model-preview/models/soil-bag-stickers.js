// Opaque, replaceable label art. Alpha in imported images is flattened over labelColor;
// no model-specific alpha/cutout exception is added to the shared pixel pipeline.
export function drawSoilBagSticker(canvas,settings,image=null){
  const ctx=canvas.getContext('2d'),w=canvas.width,h=canvas.height;
  ctx.clearRect(0,0,w,h);ctx.fillStyle=settings.labelColor;ctx.fillRect(0,0,w,h);
  if(settings.stickerDesign==='custom'&&image){
    const scale=Math.min((w-36)/image.width,(h-36)/image.height);
    const width=image.width*scale,height=image.height*scale;
    ctx.drawImage(image,(w-width)/2,(h-height)/2,width,height);return;
  }
  const ink=settings.inkColor;
  ctx.strokeStyle=ink;ctx.fillStyle=ink;ctx.lineWidth=4;
  ctx.strokeRect(18,18,w-36,h-36);
  ctx.textAlign='center';ctx.textBaseline='middle';
  ctx.font='600 27px system-ui, sans-serif';ctx.fillText('RE : FLORA',w/2,60);
  ctx.font='500 14px system-ui, sans-serif';ctx.fillText('G A R D E N   S E R I E S',w/2,98);
  ctx.save();ctx.translate(w/2,225);
  const leaf=(x,y,rotation,scale=1)=>{
    ctx.save();ctx.translate(x,y);ctx.rotate(rotation);ctx.scale(scale,scale);
    ctx.beginPath();ctx.moveTo(0,0);ctx.bezierCurveTo(-58,-15,-73,-75,-9,-88);ctx.bezierCurveTo(24,-58,26,-20,0,0);ctx.fill();ctx.restore();
  };
  const design=settings.stickerDesign==='custom'?'sprout':settings.stickerDesign;
  if(design==='flower'){
    ctx.beginPath();ctx.moveTo(0,58);ctx.quadraticCurveTo(-14,18,0,-25);ctx.stroke();
    for(let i=0;i<5;i++){ctx.save();ctx.rotate(i*Math.PI*2/5);ctx.beginPath();ctx.ellipse(0,-67,22,45,0,0,Math.PI*2);ctx.fill();ctx.restore();}
    ctx.fillStyle=settings.labelColor;ctx.beginPath();ctx.arc(0,-23,13,0,Math.PI*2);ctx.fill();ctx.fillStyle=ink;
    leaf(-3,35,-.45,.58);leaf(5,40,1.55,.48);
  }else if(design==='layers'){
    for(let i=0;i<3;i++){
      ctx.globalAlpha=1-i*.16;ctx.beginPath();ctx.moveTo(-117,18+i*29);ctx.quadraticCurveTo(-32,4+i*29,31,23+i*29);ctx.quadraticCurveTo(79,37+i*29,117,17+i*29);ctx.lineTo(117,39+i*29);ctx.quadraticCurveTo(42,55+i*29,-117,41+i*29);ctx.closePath();ctx.fill();
    }ctx.globalAlpha=1;
    ctx.lineWidth=7;ctx.beginPath();ctx.moveTo(0,55);ctx.lineTo(0,-30);ctx.stroke();leaf(0,-5,-.70,.65);leaf(0,-14,1.10,.61);
  }else{
    ctx.lineWidth=7;ctx.beginPath();ctx.moveTo(0,58);ctx.quadraticCurveTo(8,11,0,-32);ctx.stroke();leaf(0,-17,-.55,.84);leaf(0,-20,1.16,.74);
    ctx.lineWidth=3;for(let i=-2;i<=2;i++){ctx.beginPath();ctx.moveTo(0,57);ctx.quadraticCurveTo(i*22,72,i*34,99);ctx.stroke();}
    ctx.beginPath();ctx.moveTo(-106,55);ctx.quadraticCurveTo(0,42,106,55);ctx.stroke();
  }
  ctx.restore();
  ctx.font='700 66px "Noto Sans CJK SC", system-ui, sans-serif';ctx.fillText('营养土',w/2,386);
  ctx.font='500 20px system-ui, sans-serif';ctx.fillText(design==='flower'?'FLOWER MIX':design==='layers'?'SEEDLING MIX':'NUTRIENT SOIL',w/2,435);
  ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(72,470);ctx.lineTo(w-72,470);ctx.stroke();
  ctx.font='500 17px system-ui, sans-serif';ctx.fillText('GROW SLOW · ROOT DEEP',w/2,505);
  ctx.font='600 23px system-ui, sans-serif';ctx.fillText('2 L',w/2,565);
}
