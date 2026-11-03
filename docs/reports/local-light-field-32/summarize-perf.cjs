const fs=require('node:fs');
const median=a=>{a.sort((a,b)=>a-b);if(!a.length)return null;const mid=Math.floor(a.length/2);return a.length%2?a[mid]:(a[mid-1]+a[mid])/2;};
function time(line){const m=line.match(/^\[(\d\d):(\d\d):(\d\d)\.(\d\d\d)/);return m?((+m[1]*60+ +m[2])*60+ +m[3])*1000+ +m[4]:NaN;}
const result={};
for(const mode of ['original','irradiance','connections']){
 const s=fs.readFileSync(`target/shared-light-32/final-${mode}-32.log`,'utf8');
 const lines=s.split('\n');
 const frames=lines.map(line=>{const m=line.match(/\[PERF\]\[FRAME\] frame (\d+) total ([\d.]+)ms/);return m?{n:+m[1],ms:time(line),duration:+m[2]}:null;}).filter(Boolean);
 const end=frames.at(-1).ms,start=end-6000;
 const tail=frames.filter(f=>f.ms>=start),first=tail[0],last=tail.at(-1);
 const scopes=lines.filter(l=>l.includes('[GPU_FRAME_SCOPE]')&&time(l)>=start&&time(l)<=end);
 const gpu={};
 for(const key of ['frame.render','graphics.flora_lighting_cache','graphics.local_light_field','ddgi.probe_trace']){
  const values=scopes.map(l=>{const m=l.match(new RegExp(key.replaceAll('.','\\.')+'=(\\d+)us'));return m?+m[1]/1000:null;}).filter(v=>v!==null);
  gpu[key]={samples:values.length,medianMs:median(values)};
 }
 result[mode]={elapsedMs:last.ms-first.ms,frameAdvancement:last.n-first.n,actualFps:1000*(last.n-first.n)/(last.ms-first.ms),gpu};
}
fs.writeFileSync('target/shared-light-32/final-perf.json',JSON.stringify(result,null,2));
console.log(JSON.stringify(result,null,2));
