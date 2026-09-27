import {projectedCoverage} from './connectivity.mjs';

// Three r183 RGBADepthPacking's UnpackFactors4, applied to byte values.
export function unpackDepth(bytes,index){
  return bytes[index]/256+bytes[index+1]/65536+bytes[index+2]/16777216+bytes[index+3]/(255*16777216);
}
export function tileDepth(original,rgba,packed,groups,size){
  const result=new Float32Array(size*size).fill(1);
  const coverage=groups.map(group=>projectedCoverage(group.triangles,size));
  for(let i=0;i<result.length;i++){
    if(!rgba[i*4+3])continue;
    if(original[i*4+3])result[i]=unpackDepth(packed,i*4);
    else {
      const nearest=Math.min(...coverage.map(group=>group.depth[i]));
      if(!Number.isFinite(nearest))throw new Error('Repaired part pixel has no geometric depth');
      result[i]=Math.max(0,Math.min(1,nearest*.5+.5));
    }
  }
  return result;
}
