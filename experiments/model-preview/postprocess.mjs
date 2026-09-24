import {projectedCoverage,repairCoverage,labelComponents} from './connectivity.mjs';

const linear=v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4;
const srgb=v=>v<=.0031308?v*12.92:1.055*v**(1/2.4)-.055;

// Reconstruct only missing colors from same-group bridge endpoints. This is an
// image repair, not a second material renderer or an estimate of new lighting.
export function repairImage(rgba, owners, groups, size, sampleColor=()=>null) {
  const result=rgba.slice(), chosenDepth=new Float64Array(size*size).fill(Infinity),chosenGroup=owners.slice();
  const stats=[];
  for(const {id,triangles,fallbackColor=[180,180,180]} of groups) {
    const original=Uint8Array.from({length:size*size},(_,i)=>rgba[i*4+3]>0&&owners[i]===id);
    const coverage=projectedCoverage(triangles,size);
    const support={shares(a,b){
      if((rgba[a*4+3]&&!original[a]) || (rgba[b*4+3]&&!original[b])) return false;
      return coverage.shares(a,b);
    }};
    const repair=repairCoverage(original,support,size), colors=rgba.slice(), filled=original.slice();
    for(const path of repair.bridges) {
      const first=path[0]*4,last=path.at(-1)*4;
      const start=[0,1,2].map(c=>linear(colors[first+c]/255));
      const end=[0,1,2].map(c=>linear(colors[last+c]/255));
      path.forEach((pixel,i)=>{
        if(filled[pixel]) return;
        const t=i/(path.length-1);
        for(let c=0;c<3;c++) colors[pixel*4+c]=Math.round(255*srgb(start[c]*(1-t)+end[c]*t));
        colors[pixel*4+3]=255;filled[pixel]=1;
      });
    }
    for(let i=0;i<original.length;i++) if(repair.additions[i]&&!rgba[i*4+3]&&coverage.depth[i]<chosenDepth[i]) {
      chosenDepth[i]=coverage.depth[i];chosenGroup[i]=id;result.set(colors.subarray(i*4,i*4+4),i*4);
    }
    // Connectivity alone cannot create a component that missed every pixel
    // center. Give every projected surface a conservative pixel footprint;
    // original samples and nearer groups retain priority.
    const samples=Array.from({length:original.length},(_,i)=>i).filter(i=>original[i]);
    let preserved=0;
    for(let i=0;i<original.length;i++) if(!rgba[i*4+3]&&coverage.has(i)&&coverage.depth[i]<=chosenDepth[i]){
      let color=sampleColor(id,i,size);
      if(!color){
        const nearest=samples.reduce((best,p)=>{
          const d=(p%size-i%size)**2+(Math.floor(p/size)-Math.floor(i/size))**2;
          return d<best.distance?{pixel:p,distance:d}:best;
        },{pixel:-1,distance:Infinity}).pixel;
        color=nearest<0?fallbackColor:Array.from(rgba.subarray(nearest*4,nearest*4+3));
      }
      chosenDepth[i]=coverage.depth[i];chosenGroup[i]=id;result.set([...color,255],i*4);
      if(!repair.additions[i])preserved++;
    }
    stats.push({id,before:repair.before,after:repair.after,added:repair.added,preserved});
  }
  let added=0;
  for(let i=0;i<owners.length;i++) if(!rgba[i*4+3]&&result[i*4+3]) added++;
  for(const group of stats){
    const visible=Uint8Array.from(chosenGroup,(id,i)=>id===group.id&&result[i*4+3]?1:0);
    group.after=labelComponents(visible,size).count;
  }
  return {rgba:result,added,groups:stats};
}

export function quantizeImage(rgba,steps) {
  if(!steps) return rgba;
  const result=rgba.slice();
  for(let i=0;i<rgba.length;i+=4) if(rgba[i+3]) {
    const rgb=[0,1,2].map(c=>linear(rgba[i+c]/255));
    const luminance=rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;
    const scale=luminance>0?Math.round(luminance*steps)/steps/luminance:0;
    for(let c=0;c<3;c++) result[i+c]=Math.round(255*srgb(Math.min(1,rgb[c]*scale)));
  }
  return result;
}
