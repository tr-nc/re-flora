import test from 'node:test';
import assert from 'node:assert/strict';
import {soilBagStickerArt} from '../models/soil-bag-sticker-art.mjs';
import {drawSoilBagSticker} from '../models/soil-bag-stickers.js';
import {soilBagDefaults} from '../models/soil-bag-geometry.mjs';

const designs=['sprout','layers','flower'];
const inside=(x,y,points)=>{
  let hit=false;
  for(let i=0,j=points.length-1;i<points.length;j=i++){
    const [a,b]=points[i],[c,d]=points[j];
    if((b>y)!==(d>y)&&x<(c-a)*(y-b)/(d-b)+a)hit=!hit;
  }
  return hit;
};
// Test-only flattening for silhouette sampling; production draws native Canvas curves.
const sampledArt=design=>soilBagStickerArt(design).shapes.map(shape=>{
  if(shape.kind!=='path')return shape;
  const points=[];let last;
  for(const [method,...args] of shape.commands){
    if(method==='moveTo'||method==='lineTo'){last=args;points.push(last);}
    else if(method==='bezierCurveTo'){
      const start=last;
      for(let i=1;i<=32;i++){
        const t=i/32,s=1-t;
        points.push([0,1].map(axis=>s**3*start[axis]+3*s*s*t*args[axis]+3*s*t*t*args[axis+2]+t**3*args[axis+4]));
      }
      last=args.slice(4);
    }
  }
  return {...shape,points};
});
const coverage=(shapes,x,y)=>{
  let ink=0;
  for(const shape of shapes){
    let hit;
    if(shape.kind==='path')hit=inside(x,y,shape.points);
    else if(shape.kind==='rect')hit=x>=shape.x&&x<shape.x+shape.width&&y>=shape.y&&y<shape.y+shape.height;
    else{
      const dx=x-shape.x,dy=y-shape.y,c=Math.cos(shape.rotation),s=Math.sin(shape.rotation);
      hit=((dx*c+dy*s)/shape.rx)**2+((-dx*s+dy*c)/shape.ry)**2<=1;
    }
    if(hit)ink=shape.fill==='ink'?1:0;
  }
  return ink;
};

test('packaging artwork is deterministic, continuous, finite and retains one large mark and a solid title block',()=>{
  for(const design of designs){
    const art=soilBagStickerArt(design);
    assert.deepEqual(art,soilBagStickerArt(design));assert.deepEqual(art.viewBox,{width:80,height:100});
    assert.ok(art.shapes.every(shape=>['path','ellipse','rect'].includes(shape.kind)&&['ink','background'].includes(shape.fill)));
    assert.ok(art.shapes.flatMap(shape=>shape.kind==='path'?shape.commands.flatMap(command=>command.slice(1)):Object.values(shape).filter(v=>typeof v==='number')).every(Number.isFinite));
    assert.deepEqual(art.shapes.at(-1),{kind:'rect',x:10,y:85,width:60,height:7.5,fill:'ink'});
    const shapes=sampledArt(design),count=Array.from({length:1280},(_,i)=>coverage(shapes,(i%32+.5)*2.5,(Math.floor(i/32)+.5)*2.5)).reduce((sum,v)=>sum+v,0);
    assert.ok(count>280&&count<650,'a substantial, uncluttered silhouette, without relying on tiny lettering');
  }
  assert.deepEqual(soilBagStickerArt('custom'),soilBagStickerArt());
  assert.deepEqual(soilBagStickerArt('unknown'),soilBagStickerArt());
});

test('continuous large marks remain distinct in tiny 8 by 10 center samples without pre-pixelizing the artwork',()=>{
  const samples=designs.map(design=>{
    const shapes=sampledArt(design);
    return Array.from({length:80},(_,i)=>coverage(shapes,(i%8+.5)*10,(Math.floor(i/8)+.5)*10));
  });
  for(let a=0;a<samples.length;a++)for(let b=a+1;b<samples.length;b++){
    const difference=samples[a].filter((v,i)=>v!==samples[b][i]).length;
    assert.ok(difference>=12,'different glyphs must differ by broad shape, not tiny text or palette alone');
  }
});

test('source artwork uses native-size vector curves, not coarse pixel stamping or lettering',()=>{
  for(const stickerDesign of designs){
    const calls=[],ctx=new Proxy({},{get(target,key){
      if(key in target)return target[key];
      return (...args)=>calls.push({key,args,color:target.fillStyle});
    }});
    drawSoilBagSticker({width:512,height:640,getContext:()=>ctx},{...soilBagDefaults,stickerDesign});
    assert.equal(calls.filter(call=>/Text$/.test(call.key)).length,0,'unreadable lettering must remain removed');
    assert.ok(calls.some(call=>['bezierCurveTo','ellipse'].includes(call.key)),'source silhouettes must use continuous curves before shared pixel processing');
    assert.equal(calls.filter(call=>call.key==='fillRect').length,1,'only the opaque background is filled as a rectangle; no coarse raster grid');
    assert.deepEqual(calls.find(call=>call.key==='scale').args,[6.4,6.4],'view-box units scale directly to the actual texture');
    assert.ok(!calls.some(call=>['stroke','strokeText','drawImage','putImageData'].includes(call.key)),'no tiny outlines or pre-rasterized image enlargement');
    assert.ok(calls.some(call=>call.key==='fill'&&call.color===soilBagDefaults.inkColor));
  }
});

test('image imports are aspect-fitted with nearest resampling after an opaque background fill',()=>{
  const calls=[],ctx={imageSmoothingEnabled:true,clearRect(...args){calls.push(['clearRect',...args]);},fillRect(...args){calls.push(['fillRect',...args]);},drawImage(...args){calls.push(['drawImage',...args]);}};
  const image={width:8,height:4};
  drawSoilBagSticker({width:512,height:640,getContext:()=>ctx},{...soilBagDefaults,stickerDesign:'custom'},image);
  assert.equal(ctx.imageSmoothingEnabled,false,'nearest GPU filtering cannot undo a blurred canvas import');
  assert.equal(ctx.fillStyle,soilBagDefaults.labelColor);
  assert.equal(calls[1][0],'fillRect');assert.equal(calls[2][0],'drawImage');
  assert.equal(calls[2][4]/calls[2][5],2,'retain original image aspect ratio');
  assert.ok(calls[2][2]>=0&&calls[2][3]>=0);
});
