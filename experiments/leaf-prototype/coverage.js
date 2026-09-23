// Prototype-only conservative rasterization for the fixed orthographic viewer.
// One screen-space bounding quad per source triangle; an exact triangle/square
// separating-axis test decides coverage. No supersampling or image dilation.
import * as THREE from 'three';

export function coverageGeometry(source) {
  const names = ['A', 'B', 'C'];
  const data = { position: [] };
  for (const name of names) {
    data[`point${name}`] = [];
    data[`normal${name}`] = [];
    data[`uv${name}`] = [];
  }
  const corners = [[0,0], [1,0], [1,1], [0,0], [1,1], [0,1]];
  for (let i = 0; i < source.index.count; i += 3) {
    for (const corner of corners) {
      data.position.push(...corner, 0);
      for (let j = 0; j < 3; j++) {
        const index = source.index.getX(i+j), name = names[j];
        for (const [attribute, prefix, size] of [['position','point',3], ['normal','normal',3], ['uv','uv',2]]) {
          const values = source.getAttribute(attribute);
          for (let k = 0; k < size; k++) data[`${prefix}${name}`].push(values.array[index*size+k]);
        }
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  for (const [name, values] of Object.entries(data)) {
    geometry.setAttribute(name, new THREE.Float32BufferAttribute(values, name.startsWith('uv') ? 2 : 3));
  }
  return geometry;
}

const varyings = `
  varying vec3 screenA, screenB, screenC;
  varying vec3 worldA, worldB, worldC;
  varying vec3 shadingA, shadingB, shadingC;
  varying vec2 texA, texB, texC;
`;

export function coverageMaterial(uniforms, leafShading) {
  return new THREE.ShaderMaterial({
    uniforms,
    vertexShader: `
      uniform float tileResolution;
      attribute vec3 pointA, pointB, pointC;
      attribute vec3 normalA, normalB, normalC;
      attribute vec2 uvA, uvB, uvC;
      ${varyings}
      vec3 projectPoint(vec3 p) {
        vec4 clip = projectionMatrix * modelViewMatrix * vec4(p,1.);
        vec3 ndc = clip.xyz / clip.w;
        return vec3((ndc.xy*.5+.5)*tileResolution, ndc.z);
      }
      void main() {
        screenA=projectPoint(pointA); screenB=projectPoint(pointB); screenC=projectPoint(pointC);
        worldA=(modelMatrix*vec4(pointA,1.)).xyz;
        worldB=(modelMatrix*vec4(pointB,1.)).xyz;
        worldC=(modelMatrix*vec4(pointC,1.)).xyz;
        shadingA=normalize(mat3(modelMatrix)*normalA);
        shadingB=normalize(mat3(modelMatrix)*normalB);
        shadingC=normalize(mat3(modelMatrix)*normalC);
        texA=uvA; texB=uvB; texC=uvC;
        // Enclose candidate fragment centers with integer quad boundaries.
        // Fractional bounds can snap inward on the hardware subpixel grid and
        // suppress a valid fragment before the exact coverage test ever runs.
        vec2 lo=floor(min(screenA.xy,min(screenB.xy,screenC.xy))-.5001);
        vec2 hi=ceil(max(screenA.xy,max(screenB.xy,screenC.xy))+.5001);
        vec2 pixel=mix(lo,hi,position.xy);
        gl_Position=vec4(pixel/tileResolution*2.-1.,0.,1.);
      }`,
    fragmentShader: `
      ${varyings}
      ${leafShading}
      float cross2(vec2 a,vec2 b) { return a.x*b.y-a.y*b.x; }
      bool outsideEdge(vec2 a,vec2 b,vec2 center,float winding) {
        vec2 edge=b-a;
        float distance=winding*cross2(edge,center-a);
        // Projection radius of an axis-aligned half-pixel square on edge normal.
        float radius=.5001*(abs(edge.x)+abs(edge.y));
        return distance < -radius;
      }
      vec3 onSegment(vec2 p,vec2 a,vec2 b,vec3 wa,vec3 wb) {
        vec2 edge=b-a;
        float t=clamp(dot(p-a,edge)/max(dot(edge,edge),1.e-20),0.,1.);
        return mix(wa,wb,t);
      }
      vec2 atWeights(vec3 w) { return w.x*screenA.xy+w.y*screenB.xy+w.z*screenC.xy; }
      vec3 sampleWeights(vec2 p,float area) {
        if(abs(area)>1.e-10) {
          vec3 w=vec3(cross2(screenB.xy-p,screenC.xy-p),
                      cross2(screenC.xy-p,screenA.xy-p),
                      cross2(screenA.xy-p,screenB.xy-p))/area;
          if(min(w.x,min(w.y,w.z))>=0.) return w;
        }
        // For newly covered pixels, shade the nearest actual point on the
        // triangle, not extrapolated UVs/normals outside its silhouette.
        vec3 ab=onSegment(p,screenA.xy,screenB.xy,vec3(1,0,0),vec3(0,1,0));
        vec3 bc=onSegment(p,screenB.xy,screenC.xy,vec3(0,1,0),vec3(0,0,1));
        vec3 ca=onSegment(p,screenC.xy,screenA.xy,vec3(0,0,1),vec3(1,0,0));
        vec3 best=dot(atWeights(ab)-p,atWeights(ab)-p)<dot(atWeights(bc)-p,atWeights(bc)-p)?ab:bc;
        return dot(atWeights(best)-p,atWeights(best)-p)<dot(atWeights(ca)-p,atWeights(ca)-p)?best:ca;
      }
      void main() {
        vec2 p=gl_FragCoord.xy;
        vec2 lo=min(screenA.xy,min(screenB.xy,screenC.xy));
        vec2 hi=max(screenA.xy,max(screenB.xy,screenC.xy));
        float area=cross2(screenB.xy-screenA.xy,screenC.xy-screenA.xy);
        float winding=area>=0.?1.:-1.;
        // Triangle edge normals + box X/Y axes form the full SAT test. This
        // also keeps edge-on projected segments instead of throwing them away.
        bool covered=!any(lessThan(p+vec2(.5001),lo)) && !any(greaterThan(p-vec2(.5001),hi))
          && !outsideEdge(screenA.xy,screenB.xy,p,winding)
          && !outsideEdge(screenB.xy,screenC.xy,p,winding)
          && !outsideEdge(screenC.xy,screenA.xy,p,winding);
        vec3 w=sampleWeights(p,area);
        // Evaluate derivatives before discard, including helper invocations.
        vec4 color=shadeLeaf(w.x*texA+w.y*texB+w.z*texC,
          w.x*shadingA+w.y*shadingB+w.z*shadingC,
          w.x*worldA+w.y*worldB+w.z*worldC,area>=0.);
        if(!covered) discard;
        gl_FragDepth=(w.x*screenA.z+w.y*screenB.z+w.z*screenC.z)*.5+.5;
        gl_FragColor=color;
        #include <colorspace_fragment>
      }`,
  });
}
