import {projectedCoverage,repairCoverage} from './connectivity.mjs';

const linear=v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4;
const srgb=v=>v<=.0031308?v*12.92:1.055*v**(1/2.4)-.055;

// Reconstruct only missing colors from same-group bridge endpoints. This is an
// image repair, not a second material renderer or an estimate of new lighting.
export function repairImage(rgba, owners, groups, size) {
  const result=rgba.slice(), chosenDepth=new Float64Array(size*size).fill(Infinity);
  const stats=[];
  for(const {id,triangles,preserve=[]} of groups) {
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
      chosenDepth[i]=coverage.depth[i];result.set(colors.subarray(i*4,i*4+4),i*4);
    }
    // Connectivity repair cannot seed a feature that missed every pixel center.
    // Conservatively cover only explicitly marked thin geometry, without painting
    // over center-sampled pixels or another group's closer preserved feature.
    let preserved=0;
    for(const feature of preserve){
      const footprint=projectedCoverage(feature.triangles,size);
      if(original.some((visible,i)=>visible&&footprint.has(i))) continue;
      for(let i=0;i<original.length;i++) if(!rgba[i*4+3]&&footprint.has(i)&&footprint.depth[i]<chosenDepth[i]){
        chosenDepth[i]=footprint.depth[i];result.set([...feature.color,255],i*4);preserved++;
      }
    }
    stats.push({id,before:repair.before,after:repair.after,added:repair.added,preserved});
  }
  let added=0;
  for(let i=0;i<owners.length;i++) if(!rgba[i*4+3]&&result[i*4+3]) added++;
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
