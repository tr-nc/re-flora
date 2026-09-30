import {maskPresets,maskValue,resolvePalette,paintMask,validateMask,defaultPalette} from '../../assets/models/palette-mask.mjs';

class PaletteMaskEditor extends HTMLElement{
  constructor(){
    super();this.attachShadow({mode:'open'});this._value='root-tip';this._palette=defaultPalette;this.history=[];this.generation=0;
    this.shadowRoot.innerHTML=`<style>
      :host{display:block;margin:10px 0;color:inherit;font:inherit}canvas{display:block;width:100%;max-width:256px;aspect-ratio:1;image-rendering:pixelated;touch-action:none;border:1px solid #77816b;cursor:crosshair}button,select,input{font:inherit;max-width:100%;box-sizing:border-box}button,select{background:#26372c;color:#f0ead6;border:1px solid #6e866e;padding:5px}label{display:block;margin:6px 0}p{font-size:12px;line-height:1.5} .row{display:flex;gap:5px;flex-wrap:wrap}input[type=file]{width:100%}output{display:block;font-size:12px;color:#ffc49b}button[aria-pressed=true]{outline:2px solid #e6c979}
      </style><label>权重模板 <select aria-label="权重模板"></select></label><div class="row slots"></div>
      <label>画笔半径 <input aria-label="画笔半径" type="range" min="2" max="32" value="12"></label>
      <canvas width="128" height="128" tabindex="0" role="img" aria-label="调色板权重贴纸画布，拖动绘制；方向键移动光标，空格绘制"></canvas>
      <p>左 ¾：每片花瓣（上根下尖，左右瓣缘）。右上：花心；右下：花萼。红/绿/蓝/黑权重代表 A/B/C/D，不是最终颜色。</p>
      <div class="row"><button type="button" class="undo">撤销</button><button type="button" class="view">查看权重</button><button type="button" class="export">导出权重 PNG</button></div>
      <label>导入权重 PNG（不透明，32–512px）<input aria-label="导入权重 PNG" type="file" accept="image/png"></label><output role="status"></output>`;
    const root=this.shadowRoot,select=root.querySelector('select');
    for(const [id,label]of maskPresets)select.add(new Option(label,id));
    select.add(new Option('自定义贴纸','custom'));select.options[select.options.length-1].disabled=true;
    this.brushSlot=0;this.raw=false;this.cursor=[.5,.5];
    for(let i=0;i<4;i++){
      const b=document.createElement('button');b.type='button';b.textContent=`画 ${'ABCD'[i]}`;b.setAttribute('aria-pressed',String(i===0));
      b.addEventListener('click',()=>{this.brushSlot=i;for(const [j,button]of [...root.querySelectorAll('.slots button')].entries())button.setAttribute('aria-pressed',String(i===j));});root.querySelector('.slots').append(b);
    }
    select.addEventListener('input',e=>{e.stopPropagation();this.remember();this._value=select.value;this.changed();});
    root.querySelector('input[type=range]').addEventListener('input',e=>e.stopPropagation());
    root.querySelector('.undo').addEventListener('click',()=>{if(this.history.length){this._value=this.history.pop();this.changed();}});
    root.querySelector('.view').addEventListener('click',()=>{this.raw=!this.raw;root.querySelector('.view').textContent=this.raw?'查看配色':'查看权重';this.draw();});
    root.querySelector('.export').addEventListener('click',()=>{
      const mask=maskValue(this._value),canvas=document.createElement('canvas');canvas.width=mask.width;canvas.height=mask.height;
      canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(mask.pixels),mask.width,mask.height),0,0);
      canvas.toBlob(blob=>{if(!blob)return;const url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download='flower-palette-weights.png';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);},'image/png');
    });
    root.querySelector('input[type=file]').addEventListener('change',async e=>{
      e.stopPropagation();const input=e.currentTarget,file=input.files[0],generation=++this.generation;if(!file)return;
      try{
        if(file.size>4*1024*1024)throw new Error('PNG 不能超过 4 MiB');
        const bytes=new Uint8Array(await file.slice(0,32).arrayBuffer());
        if(bytes.length<24||![137,80,78,71,13,10,26,10].every((v,i)=>bytes[i]===v))throw new Error('仅接受 PNG 权重图');
        const header=new DataView(bytes.buffer),width=header.getUint32(16),height=header.getUint32(20);
        if(width<32||height<32||width>512||height>512)throw new Error('宽高必须为 32–512px');
        const image=await createImageBitmap(file,{colorSpaceConversion:'none',premultiplyAlpha:'none'});
        const canvas=document.createElement('canvas');canvas.width=width;canvas.height=height;
        canvas.getContext('2d').drawImage(image,0,0);image.close();
        const mask=validateMask({width,height,pixels:new Uint8Array(canvas.getContext('2d').getImageData(0,0,width,height).data)});
        if(!this.isConnected||generation!==this.generation)return;
        this.remember();this._value=mask;this.changed();root.querySelector('output').textContent='已导入权重；未上传网络。';
      }catch(error){if(this.isConnected&&generation===this.generation)root.querySelector('output').textContent=error.message;}
      input.value='';
    });
    const canvas=root.querySelector('canvas');
    const paint=e=>{const rect=canvas.getBoundingClientRect();this.cursor=[Math.max(0,Math.min(1,(e.clientX-rect.left)/rect.width)),Math.max(0,Math.min(1,(e.clientY-rect.top)/rect.height))];this.paint();};
    canvas.addEventListener('pointerdown',e=>{if(e.button!==0)return;e.preventDefault();this.remember();this.dragging=true;canvas.setPointerCapture(e.pointerId);paint(e);});
    canvas.addEventListener('pointermove',e=>{if(this.dragging)paint(e);});
    for(const type of ['pointerup','pointercancel','lostpointercapture'])canvas.addEventListener(type,()=>this.dragging=false);
    canvas.addEventListener('keydown',e=>{
      const delta={ArrowLeft:[-.02,0],ArrowRight:[.02,0],ArrowUp:[0,-.02],ArrowDown:[0,.02]}[e.key];
      if(delta){e.preventDefault();e.stopPropagation();this.cursor=this.cursor.map((v,i)=>Math.max(0,Math.min(1,v+delta[i])));root.querySelector('output').textContent=`光标 ${this.cursor.map(v=>Math.round(v*100)).join(', ')}%；空格绘制`;}
      if(e.key===' '){e.preventDefault();e.stopPropagation();this.remember();this.paint();}
    });
  }
  connectedCallback(){this.draw();}
  disconnectedCallback(){this.generation++;}
  get value(){return this._value;}
  set value(value){maskValue(value);this._value=value;this.history=[];this.draw();}
  set palette(colors){this._palette=colors;this.draw();}
  remember(){this.history.push(this._value);if(this.history.length>12)this.history.shift();}
  paint(){const mask=maskValue(this._value),radius=Number(this.shadowRoot.querySelector('input[type=range]').value)*mask.width/128;this._value=paintMask(mask,this.cursor[0]*mask.width,this.cursor[1]*mask.height,this.brushSlot,radius);this.changed();}
  changed(){this.generation++;this.draw();this.dispatchEvent(new Event('input',{bubbles:true}));}
  draw(){
    const mask=maskValue(this._value),canvas=this.shadowRoot.querySelector('canvas');canvas.width=mask.width;canvas.height=mask.height;
    canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(this.raw?mask.pixels:resolvePalette(mask,this._palette)),mask.width,mask.height),0,0);
    this.shadowRoot.querySelector('select').value=typeof this._value==='string'?this._value:'custom';
    this.shadowRoot.querySelector('.undo').disabled=!this.history.length;
  }
}
customElements.define('palette-mask-editor',PaletteMaskEditor);
