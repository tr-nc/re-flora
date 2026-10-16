// Test-only WebGL raster reference. CPU projection uses doubles; the GPU uses
// float32. Near a half-subpixel tie either adjacent snapped coordinate is valid.
// Enumerate those alternatives so only samples touched by ambiguous triangles
// are excluded from the exact-depth comparison. Never alter rendered coverage,
// depth, the depth-error threshold, or original-RGBA checks.
export function rasterTriangleVariants(triangle,size,subpixelScale){
  const vertices=triangle.map(([x,y,z])=>{
    const options=v=>{
      const scaled=v*subpixelScale,rounded=Math.round(scaled);
      // Four float32 ulps at tile scale bound the projection-rounding ambiguity.
      const uncertainty=4*2**-23*Math.max(size,Math.abs(v));
      const tied=Math.abs(scaled-(Math.floor(scaled)+.5))<=uncertainty*subpixelScale;
      return tied?[Math.floor(scaled)/subpixelScale,Math.ceil(scaled)/subpixelScale]:[rounded/subpixelScale];
    };
    return options(x).flatMap(a=>options(y).map(b=>[a,b,z]));
  });
  return vertices[0].flatMap(a=>vertices[1].flatMap(b=>vertices[2].map(c=>[a,b,c])));
}

export function rasterDepthAt(triangles,x,y){
  let depth=Infinity,boundary=false,stableDepth=Infinity,ambiguousDepth=Infinity;
  for(const variants of triangles)for(const [a,b,c]of variants){
    const area=(b[1]-c[1])*(a[0]-c[0])+(c[0]-b[0])*(a[1]-c[1]);
    if(Math.abs(area)<1e-10)continue;
    const u=((b[1]-c[1])*(x-c[0])+(c[0]-b[0])*(y-c[1]))/area;
    const v=((c[1]-a[1])*(x-c[0])+(a[0]-c[0])*(y-c[1]))/area,w=1-u-v;
    const minimum=Math.min(u,v,w);
    if(minimum>=-1e-7){
      boundary ||= minimum<1e-7;
      const z=(a[2]*u+b[2]*v+c[2]*w)*.5+.5;
      if(variants.length>1)ambiguousDepth=Math.min(ambiguousDepth,z);
      else stableDepth=Math.min(stableDepth,z);
      depth=Math.min(depth,z);
    }
  }
  return {depth,boundary,ambiguous:Number.isFinite(ambiguousDepth)&&ambiguousDepth<=stableDepth};
}
