// Plain apple pigment with smooth directional sunlight and a live sun highlight.
// Reconstructed world surface positions/normals describe illumination.
module apple_appearance;
import color;
import flora_shadow;
import environment_lighting;
import tracer_types;
import sunlight;

public float3 appleBaseColor(uint material) {
    float3 srgb=material==0u?float3(203,48,47)/255.0:
        material==1u?float3(116,80,46)/255.0:float3(77,145,62)/255.0;
    return srgbToLinear(srgb);
}

public float appleSunFacing(float3 normal,float3 sunDirection) {
    float3 n=normal*rsqrt(max(dot(normal,normal),1e-12));
    // Opaque fruit: no direct sun on the back. Indirect illumination remains
    // separate; don't bake a dark hemisphere into the red material.
    float facing=saturate(dot(n,sunDirection));
    return facing;
}

public float3 applyAppleLighting(U_GuiInput gui,U_SunInfo sun,float3 baseColor,
    float3 position,float3 normal,uint material,float3 cameraPosition,
    float sunTransmittance,float3 environmentLight) {
    environmentLight*=gui.apple_environment_fill;
    float3 color=applyStylizedVoxelLightingWithEnvironment(sun,baseColor,
        sunTransmittance*appleSunFacing(normal,sun.sun_dir),environmentLight);
    if(material==0u) {
        float3 n=normal*rsqrt(max(dot(normal,normal),1e-12));
        float3 delta=cameraPosition-position;
        float3 view=delta*rsqrt(max(dot(delta,delta),1e-12));
        float3 sum=view+sun.sun_dir;
        float3 halfVector=sum*rsqrt(max(dot(sum,sum),1e-12));
        float facing=saturate(dot(n,sun.sun_dir));
        float specular=pow(saturate(dot(n,halfVector)),24.0);
        float3 sunLight=sun.sun_color*sunLuminanceFromDirection(sun.sun_dir,sun.sun_luminance);
        color+=sunLight*sunTransmittance*specular*saturate(facing*4.0)*0.22;
    }
    return color;
}

public float3 shadeAppleSurface(U_GuiInput gui,U_SunInfo sun,U_ShadingInfo shading,
    U_ShadowCameraInfo shadowCamera,float3 position,float3 normal,
    uint material,float3 cameraPosition) {
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui,sun,shadowCamera,position,normal);
    float3 environment=sampleDiffuseEnvironment(gui,shading,position,normal);
    return applyAppleLighting(gui,sun,appleBaseColor(material),
        position,normal,material,cameraPosition,shadow.sun_transmittance,environment);
}
