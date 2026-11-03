// Disposable fixed-camera native-image regression, not a universal lighting metric.
const {execFileSync}=require('node:child_process');
const [reference,candidate]=process.argv.slice(2);
if(!reference||!candidate)throw Error('usage: node check-warmth.cjs reference.png candidate.png');
function measure(file){
 const [w,h]=execFileSync('magick',['identify','-format','%w %h',file],{encoding:'utf8'}).trim().split(' ').map(Number);
 const roi=`${Math.round(w*.24)}x${Math.round(h*.096)}+${Math.round(w*.04)}+${Math.round(h*.64)}`;
 return {file,width:w,height:h,roi,warmth:Number(execFileSync('magick',[file,'-crop',roi,'+repage','-colorspace','RGB','-format','%[fx:mean.r-mean.b]','info:'],{encoding:'utf8'}))};
}
const a=measure(reference),b=measure(candidate);if(a.width!==b.width||a.height!==b.height)throw Error('native image dimensions differ');
const ratio=b.warmth/a.warmth;console.log(JSON.stringify({reference:a,candidate:b,warmthRatio:ratio,minimumRetainedWarmth:.95},null,2));
if(!(a.warmth>0&&ratio>=.95))process.exitCode=1;
