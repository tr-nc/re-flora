import test from 'node:test';
import assert from 'node:assert/strict';
import {soilBagStickerGrid,soilBagStickerMask} from '../models/soil-bag-sticker-art.mjs';
import {drawSoilBagSticker} from '../models/soil-bag-stickers.js';
import {soilBagDefaults} from '../models/soil-bag-geometry.mjs';

const designs=['sprout','layers','flower'];

test('packaging marks are deterministic broad binary silhouettes with one solid title block',()=>{
  const {width,height}=soilBagStickerGrid;
  assert.deepEqual({width,height},{width:32,height:40});
  for(const design of designs){
    const mask=soilBagStickerMask(design);
    assert.deepEqual(mask,soilBagStickerMask(design));assert.equal(mask.length,width*height);
    assert.ok(mask.every(value=>value===0||value===1),'no gradients or antialiased palette mixtures');
    const count=mask.reduce((sum,v)=>sum+v,0);
    assert.ok(count>280&&count<650,'hero image must occupy a substantial, not overcrowded area');
    for(let y=34;y<37;y++)for(let x=4;x<28;x++)assert.equal(mask[y*width+x],1);
    for(let x=0;x<width;x++){assert.equal(mask[x],0);assert.equal(mask[(height-1)*width+x],0);}
  }
  assert.deepEqual(soilBagStickerMask('custom'),soilBagStickerMask());
  assert.deepEqual(soilBagStickerMask('unknown'),soilBagStickerMask());
});

test('the three large marks remain distinct when center-sampled down to a tiny 8 by 10 label',()=>{
  const {width}=soilBagStickerGrid;
  const samples=designs.map(design=>{
    const mask=soilBagStickerMask(design);
    return Array.from({length:80},(_,i)=>mask[(Math.floor(i/8)*4+2)*width+(i%8)*4+2]);
  });
  for(let a=0;a<samples.length;a++)for(let b=a+1;b<samples.length;b++){
    const difference=samples[a].filter((v,i)=>v!==samples[b][i]).length;
    assert.ok(difference>=12,'different glyphs must differ by a broad shape, not tiny text or palette alone');
  }
});

test('built-in drawing uses only solid rectangles and contains no text, strokes or fractional edges',()=>{
  for(const stickerDesign of designs){
    const calls=[],ctx=new Proxy({},{get(target,key){
      if(key in target)return target[key];
      return (...args)=>calls.push({key,args,color:target.fillStyle});
    }});
    drawSoilBagSticker({width:512,height:640,getContext:()=>ctx},{...soilBagDefaults,stickerDesign});
    assert.equal(calls.filter(call=>/Text$/.test(call.key)).length,0,'unreadable lettering must be removed, not shrunk');
    assert.ok(calls.every(call=>['clearRect','fillRect'].includes(call.key)),'no fine outlines, curves, fonts or compositing tricks');
    assert.ok(calls.every(call=>call.args.every(Number.isInteger)),'authored grid must upscale without antialiasing');
    assert.ok(calls.filter(call=>call.key==='fillRect'&&call.color===soilBagDefaults.inkColor).length>10);
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
