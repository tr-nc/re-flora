// Throwaway art-direction study, not production UI. No external scripts or storage.
const $ = s => document.querySelector(s);
const canvas = $('#preview'), ctx = canvas.getContext('2d');
const W = 960, H = 576, CELL = 96;
const palettes = {
  forest: {name:'藏绿 / 米黄', colors:['#233f32','#2b4937','#eee2b8','#d9b764','#a38846'], note:'最接近现版，但拿掉纯白，把亮度留给标题。'},
  moss: {name:'苔绿 / 麦黄', colors:['#374833','#40513a','#e5ddbb','#c9ac65','#9b824a'], note:'向暖绿移动；更像花圃，不像冷色程序背景。'},
  pond: {name:'夜绿 / 银白', colors:['#203e3c','#294845','#e0e4d1','#bebc75','#8c9259'], note:'更冷、更静；黄花降低饱和度，白花略带绿。'},
};
const variants = {
  A: {title:'A / 花格继续存在，标题成为其中一块。', region:[3,1,4,2], palette:'forest', motion:'turn', notes:'把中央 4×2 格并为一块藏绿，不用白底，不画封闭边框。少量格子留空或只放小叶，黄白花的占格比例比旧版更高；画面不再每处都一样满。'},
  B: {title:'B / 不靠棋盘色差，也能看出秩序。', region:[3,2,4,1], palette:'moss', motion:'sway', notes:'背景改成单色细线格，标题严格占 4×1 格，内部竖线让开文字。用成行的花和小叶建立轻重，不靠方块交替去填满画面。'},
  C: {title:'C / 像一张由小方片拼成的像素织物。', region:[2,1,6,2], palette:'pond', motion:'spark', notes:'各格之间留下细缝，标题跨 6×2 格，直接落在同色底上；框不再是一张白纸，而是布局自身的缺口。花心偶发亮点，整体更静。'},
};
const query = new URLSearchParams(location.search);
let variant = Object.hasOwn(variants,query.get('variant')) ? query.get('variant') : 'A';
let paletteKey = variants[variant].palette, motion = variants[variant].motion;
let paused = matchMedia('(prefers-reduced-motion: reduce)').matches;
let guides = false, before = false, clock = 0, last = 0, lastDraw = -1;
let cachedSprites, cachedPalette;

// Original draft silhouettes. Source reference images are never sampled here.
function flower(kind, colors, old=false) {
  const size=old?32:16, image=document.createElement('canvas');image.width=image.height=size;
  const g=image.getContext('2d'), pixels=Array(size*size).fill(null);
  const petals=kind?(old?7:6):5, mid=(size-1)/2;
  const style=old
    ? (kind?['#c8d5bb','#eeefdb','#fffdf1','#d1aa45','#f4d66e']:['#cfaa48','#f1ce62','#ffe59a','#9d773a','#d4a44d'])
    : [colors[4],kind?colors[2]:colors[3],kind?'#f2ead1':colors[3],colors[4],colors[3]];
  for(let p=0;p<petals;p++) {
    const angle=-Math.PI/2+p*Math.PI*2/petals, c=Math.cos(angle),s=Math.sin(angle);
    const length=old?4.6+.3*Math.sin(p*2):(kind?2.8:2.9);
    const breadth=old?(kind?2.3:3.5):(kind?1.85:2.2);
    const radius=old?6.4:(kind?3.8:3.9);
    for(let y=0;y<size;y++) for(let x=0;x<size;x++) {
      const dx=x-mid,dy=y-mid,u=dx*c+dy*s,v=-dx*s+dy*c;
      const d=((u-radius)/length)**2+(v/breadth)**2;
      if(d>1) continue;
      // New draft: broad flat color clusters, not a highlight on every petal.
      pixels[y*size+x]=old?(d>.77?style[0]:v<-.3?style[2]:style[1]):style[1];
    }
  }
  for(let y=0;y<size;y++) for(let x=0;x<size;x++) {
    const dx=x-mid,dy=y-mid;
    if(dx*dx+dy*dy <= (old?10:3)) pixels[y*size+x]=dx+dy<0?style[4]:style[3];
  }
  pixels.forEach((color,i)=>{if(color){g.fillStyle=color;g.fillRect(i%size,Math.floor(i/size),1,1);}});
  return image;
}
const oldSprites=[flower(0,[],true),flower(1,[],true)];
function titleRegion(){ return variants[variant].region.map(v=>v*CELL); }
function reserved(col,row){const [x,y,w,h]=variants[variant].region;return col>=x&&col<x+w&&row>=y&&row<y+h;}
function textAt(text,x,y,size,color){
  ctx.font=`500 ${size}px Pixel, monospace`;ctx.textAlign='center';ctx.textBaseline='alphabetic';ctx.fillStyle=color;
  const m=ctx.measureText(text);ctx.fillText(text,x,y+(m.actualBoundingBoxAscent-m.actualBoundingBoxDescent)/2);
}
function drawFlower(sprite,x,y,angle,extent) {
  ctx.save();ctx.translate(x,y);ctx.rotate(angle);ctx.imageSmoothingEnabled=false;
  ctx.drawImage(sprite,-extent/2,-extent/2,extent,extent);ctx.restore();
}
function angleAt(kind,col,row){
  const phase=((col+row*3)%4)*.45;
  if(motion==='turn') return Math.floor((clock+phase)/1.8)*Math.PI/12*(kind?-1:1);
  if(motion==='sway') return [-10,0,10,0][Math.floor((clock+phase)/1.2)%4]*Math.PI/180;
  return 0;
}
function drawOld(){
  const cell=144, columns=Math.ceil(W/cell)+2,rows=Math.ceil(H/cell)+2;
  const ox=Math.round((W-columns*cell)/2),oy=Math.round((H-rows*cell)/2);
  for(let r=0;r<rows;r++)for(let c=0;c<columns;c++){
    const kind=(r+c)%2,x=ox+c*cell,y=oy+r*cell;
    ctx.fillStyle=kind?'#305746':'#294f40';ctx.fillRect(x,y,cell,cell);
    const angle=Math.floor(clock/(.8/.75)+kind*.5)*Math.PI/12*(kind?-1:1);
    drawFlower(oldSprites[kind],x+cell/2,y+cell/2,angle,64);
  }
  const cy=H*.365;ctx.font='500 48px Pixel, monospace';const width=ctx.measureText('re: flora').width+60;
  ctx.fillStyle='#ffffff';ctx.fillRect((W-width)/2,cy-43,width,86);textAt('re: flora',W/2,cy,48,'#294f40');
  ctx.fillStyle='#53755b';ctx.fillRect((W-182)/2,H*.86,182,3);ctx.fillStyle='#eee5ad';ctx.fillRect((W-182)/2,H*.86,182*.62,3);
  if(guides){ctx.strokeStyle='#f28c72';ctx.lineWidth=1;for(let x=ox;x<W;x+=cell){ctx.beginPath();ctx.moveTo(x,0);ctx.lineTo(x,H);ctx.stroke();}for(let y=oy;y<H;y+=cell){ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(W,y);ctx.stroke();}}
}
function draw(){
  const ratio=canvas.width/W;ctx.setTransform(ratio,0,0,ratio,0,0);ctx.imageSmoothingEnabled=false;
  if(before){drawOld();return;}
  const colors=palettes[paletteKey].colors;
  if(cachedPalette!==paletteKey){cachedSprites=[flower(0,colors),flower(1,colors)];cachedPalette=paletteKey;}
  ctx.fillStyle=colors[0];ctx.fillRect(0,0,W,H);
  for(let row=0;row<6;row++)for(let col=0;col<10;col++){
    const x=col*CELL,y=row*CELL,kind=(col+row)%2;
    if(reserved(col,row))continue;
    ctx.fillStyle=colors[kind];
    if(variant==='A')ctx.fillRect(x,y,CELL,CELL);
    if(variant==='B'){ctx.strokeStyle=colors[1];ctx.lineWidth=1;ctx.strokeRect(x+.5,y+.5,CELL-1,CELL-1);}
    if(variant==='C')ctx.fillRect(x+3,y+3,CELL-6,CELL-6);
    const occupied=variant==='A'?(col+row*3)%5!==0:variant==='B'?(col+row)%3!==0:(col+row*2)%5!==0;
    if(!occupied){ctx.fillStyle=colors[1];ctx.fillRect(x+46,y+47,4,2);ctx.fillRect(x+48,y+44,2,3);continue;}
    drawFlower(cachedSprites[kind],x+CELL/2,y+CELL/2,angleAt(kind,col,row),variant==='C'?56:48);
    if(motion==='spark'&&Math.floor(clock/.6)%7===((col*3+row)%7)&&((col+row)%3===0)){
      ctx.fillStyle=colors[2];ctx.fillRect(x+CELL/2-1.5,y+CELL/2-1.5,3,3);
    }
  }
  const [tx,ty,tw,th]=titleRegion();
  // Not a floating overlay: the grid itself reserves these exact whole cells.
  if(variant==='B'){ctx.strokeStyle=colors[1];ctx.strokeRect(tx+.5,ty+.5,tw-1,th-1);}
  if(variant==='A'){
    ctx.fillStyle=colors[4];[[tx+12,ty+12],[tx+tw-18,ty+12],[tx+12,ty+th-14],[tx+tw-18,ty+th-14]].forEach(([x,y])=>ctx.fillRect(x,y,6,2));
  }
  textAt('re: flora',tx+tw/2,ty+th/2,variant==='C'?68:64,colors[2]);
  ctx.fillStyle=colors[1];ctx.fillRect(3*CELL,5*CELL,4*CELL,3);
  ctx.fillStyle=colors[3];ctx.fillRect(3*CELL,5*CELL,4*CELL*.62,3);
  if(guides){
    ctx.strokeStyle='#eca78388';ctx.lineWidth=1;
    for(let c=0;c<=10;c++){ctx.beginPath();ctx.moveTo(c*CELL,0);ctx.lineTo(c*CELL,H);ctx.stroke();}
    for(let r=0;r<=6;r++){ctx.beginPath();ctx.moveTo(0,r*CELL);ctx.lineTo(W,r*CELL);ctx.stroke();}
    ctx.strokeStyle='#ffce89';ctx.lineWidth=2;ctx.strokeRect(tx,ty,tw,th);
    ctx.font='11px monospace';ctx.textAlign='left';ctx.textBaseline='top';ctx.fillStyle='#ffe6bc';ctx.fillText(`${tw/CELL} × ${th/CELL} CELLS / GRID-ALIGNED`,tx+8,ty+8);
  }
}
function update(){
  document.querySelectorAll('[data-variant]').forEach(b=>b.setAttribute('aria-pressed',b.dataset.variant===variant));
  $('#paletteChoice').value=paletteKey;$('#motion').value=motion;
  $('#candidateTitle').textContent=variants[variant].title;$('#candidateNotes').textContent=variants[variant].notes;
  $('#pause').textContent=paused?'播放':'暂停';$('#pause').setAttribute('aria-pressed',paused);
  $('#before').textContent=before?'返回候选':'旧版对照';$('#before').setAttribute('aria-pressed',before);
  $('#variantName').textContent=`STUDY ${variant}`;
  const region=variants[variant].region;
  $('#geometry').textContent=before?'旧版示意 / 标题不对齐主格 / 白框为独立覆盖层':`${variant} · 10×6 主格 · 标题 ${region[2]}×${region[3]} 格 · 边界均在格线 · 花 16×16px`;
  $('#stageLabel').textContent=before?'PREVIOUS DESIGN / RECONSTRUCTION, NOT A GAME CAPTURE':'ORIGINAL STUDY / NOT IN GAME';
  const url=new URL(location.href);url.searchParams.set('variant',variant);history.replaceState(null,'',url);
  draw();
}
function choose(key){variant=key;paletteKey=variants[key].palette;motion=variants[key].motion;before=false;update();}
function move(delta){const keys=Object.keys(variants);choose(keys[(keys.indexOf(variant)+delta+3)%3]);$('#study').scrollIntoView({behavior:'instant'});}
document.querySelectorAll('[data-variant]').forEach(b=>b.onclick=()=>choose(b.dataset.variant));
$('#prev').onclick=()=>move(-1);$('#next').onclick=()=>move(1);
$('#paletteChoice').onchange=e=>{paletteKey=e.target.value;before=false;update();};
$('#motion').onchange=e=>{motion=e.target.value;before=false;update();};
$('#guides').onchange=e=>{guides=e.target.checked;draw();};
$('#pause').onclick=()=>{paused=!paused;update();};$('#before').onclick=()=>{before=!before;update();};
for(const [key,palette] of Object.entries(palettes)){
  const button=document.createElement('button');button.className='palette';
  button.innerHTML=`<b>${palette.name}</b><div class="swatches">${palette.colors.map(c=>`<i style="background:${c}"></i>`).join('')}</div><code>${palette.colors.join(' · ')}</code><small>${palette.note}</small>`;
  button.onclick=()=>{paletteKey=key;before=false;update();$('#study').scrollIntoView({behavior:'instant'});};$('#palettes').append(button);
}
document.querySelectorAll('[data-image]').forEach(b=>b.onclick=()=>{
  $('#zoomImage').src=b.dataset.image;$('#zoomImage').alt=b.dataset.caption;$('#zoomCaption').textContent=b.dataset.caption;$('#zoom').showModal();
});
$('#zoom').onclick=e=>{if(e.target===$('#zoom'))$('#zoom').close();};
function resize(){const width=canvas.getBoundingClientRect().width;canvas.width=Math.round(width*(devicePixelRatio||1));canvas.height=Math.round(canvas.width*H/W);draw();}
new ResizeObserver(resize).observe(canvas);
addEventListener('popstate',()=>{const key=new URLSearchParams(location.search).get('variant');choose(Object.hasOwn(variants,key)?key:'A');});
function tick(now){if(last&&!paused&&!document.hidden)clock+=Math.min((now-last)/1000,.1);last=now;const f=Math.floor(clock*10);if(f!==lastDraw){draw();lastDraw=f;}requestAnimationFrame(tick);}
update();document.fonts.ready.then(draw);requestAnimationFrame(tick);
