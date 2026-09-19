import * as THREE from 'three';

/** Render real geometry at N×N, then outline its alpha silhouette on the GPU.
 * No image readback or background color is involved. Surface alpha is opaque;
 * the only partial alpha is the optional outside outline.
 */
export function createPixelOutput(renderer, size) {
  const target = new THREE.WebGLRenderTarget(size, size, {
    minFilter: THREE.NearestFilter, magFilter: THREE.NearestFilter,
    depthBuffer: true, stencilBuffer: false, samples: 0,
  });
  target.texture.colorSpace = THREE.SRGBColorSpace;
  target.texture.generateMipmaps = false;
  const material = new THREE.ShaderMaterial({
    uniforms: {
      image: {value: target.texture}, texel: {value: new THREE.Vector2(1/size, 1/size)},
      enabled: {value: true}, outside: {value: true},
      edgeColor: {value: new THREE.Color('#17272e')}, edgeAlpha: {value: .85},
    },
    vertexShader: `
      varying vec2 uvPixel;
      void main() {
        uvPixel = uv;
        gl_Position = vec4(position.xy, 0.0, 1.0);
      }`,
    fragmentShader: `
      uniform sampler2D image;
      uniform vec2 texel;
      uniform bool enabled;
      uniform bool outside;
      uniform vec3 edgeColor;
      uniform float edgeAlpha;
      varying vec2 uvPixel;
      float coverage(vec2 uv) {
        // Outside the canvas is empty, not a clamped copy of its edge texel.
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) return 0.0;
        return step(0.5, texture2D(image, uv).a);
      }
      void main() {
        vec4 source = texture2D(image, uvPixel);
        vec4 result = source;
        if (enabled && edgeAlpha > 0.0) {
          float nearMax = 0.0;
          float nearMin = 1.0;
          for (int y = -1; y <= 1; y++) {
            for (int x = -1; x <= 1; x++) {
              float a = coverage(uvPixel + vec2(float(x), float(y)) * texel);
              nearMax = max(nearMax, a);
              nearMin = min(nearMin, a);
            }
          }
          if (outside && source.a < 0.5 && nearMax > 0.5) {
            result = vec4(edgeColor, edgeAlpha);
          } else if (!outside && source.a >= 0.5 && nearMin < 0.5) {
            // Overlay onto the existing opaque edge; never punch holes in it.
            result.rgb = mix(source.rgb, edgeColor, edgeAlpha);
          }
        }
        gl_FragColor = result;
        #include <colorspace_fragment>
        // Match the final RGBA8 canvas precision before premultiplication.
        // Otherwise alpha=.5 can store A=128 but a nominal red=1 rounds
        // down to premultiplied R=127, exporting a darkened red=253.
        gl_FragColor = floor(clamp(gl_FragColor, 0.0, 1.0) * 255.0 + 0.5) / 255.0;
        #include <premultiplied_alpha_fragment>
      }`,
    depthTest: false, depthWrite: false, blending: THREE.NoBlending,
    premultipliedAlpha: true, toneMapped: false,
  });
  const geometry = new THREE.PlaneGeometry(2, 2);
  const screen = new THREE.Scene();
  const quad = new THREE.Mesh(geometry, material);
  quad.frustumCulled = false;
  screen.add(quad);
  const camera = new THREE.Camera();
  return {
    resize(n) {
      target.setSize(n, n);
      material.uniforms.texel.value.set(1/n, 1/n);
    },
    setOutline({enabled, outside, color, alpha}) {
      material.uniforms.enabled.value = enabled;
      material.uniforms.outside.value = outside;
      material.uniforms.edgeColor.value.set(color);
      material.uniforms.edgeAlpha.value = alpha;
    },
    render(scene, sourceCamera) {
      renderer.setRenderTarget(target);
      renderer.clear();
      renderer.render(scene, sourceCamera);
      renderer.setRenderTarget(null);
      renderer.render(screen, camera);
    },
    dispose() { target.dispose(); geometry.dispose(); material.dispose(); },
  };
}
