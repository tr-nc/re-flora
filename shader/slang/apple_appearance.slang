// Shared apple pigment and live lighting. Canonical local coordinates describe
// peel; reconstructed world surface positions/normals describe illumination.
module apple_appearance;
import color;
import flora_shadow;
import environment_lighting;
import tracer_types;
import sunlight;

public static const uint APPLE_DIRECTIONAL_LIGHTING = 1u;
public static const uint APPLE_LIGHT_BANDS = 2u;
public static const uint APPLE_COLOR_PATCHES = 4u;
public static const uint APPLE_COLOR_STRIPES = 8u;
public static const uint APPLE_SUN_GLOSS = 16u;

public float3 appleBaseColor(uint material,float3 localPosition,uint flags) {
    float3 srgb=material==0u?float3(203,48,47)/255.0:
        material==1u?float3(116,80,46)/255.0:float3(77,145,62)/255.0;
    if(material==0u) {
        // A few broad pigment regions, never per-pixel random speckle or a
        // camera-fixed imitation of light. Both consumers sample model space.
        if((flags&APPLE_COLOR_PATCHES)!=0u) {
            float patch=sin(localPosition.x*3.1+localPosition.z*0.8)*
                sin(localPosition.y*2.3-localPosition.z*1.6+0.7);
            srgb=patch<-0.25?float3(170,34,49)/255.0:
                patch>0.25?float3(231,77,49)/255.0:srgb;
        }
        if((flags&APPLE_COLOR_STRIPES)!=0u) {
            float angle=dot(localPosition.xz,localPosition.xz)>1e-12?
                atan2(localPosition.z,localPosition.x):0.0;
            float stripe=smoothstep(0.35,0.75,sin(angle*8.0+0.7*sin(localPosition.y*4.0)));
            srgb=lerp(srgb,float3(232,142,52)/255.0,stripe*0.55);
        }
    }
    return srgbToLinear(srgb);
}

public float appleSunFacing(uint flags,float3 normal,float3 sunDirection) {
    if((flags&APPLE_DIRECTIONAL_LIGHTING)==0u)return directionWeight(normal,sunDirection);
    float3 n=normal*rsqrt(max(dot(normal,normal),1e-12));
    // Opaque fruit: no direct sun on the back. Indirect illumination remains
    // separate; don't bake a dark hemisphere into the red material.
    float facing=saturate(dot(n,sunDirection));
    if((flags&APPLE_LIGHT_BANDS)!=0u)facing=floor(facing*4.0+0.5)/4.0;
    return facing;
}

public float3 applyAppleLighting(U_GuiInput gui,U_SunInfo sun,float3 baseColor,
    float3 position,float3 normal,uint material,float3 cameraPosition,
    float sunTransmittance,float3 environmentLight) {
    uint flags=gui.apple_appearance_flags;
    if((flags&APPLE_DIRECTIONAL_LIGHTING)!=0u)environmentLight*=gui.apple_environment_fill;
    float3 color=applyStylizedVoxelLightingWithEnvironment(sun,baseColor,
        sunTransmittance*appleSunFacing(flags,normal,sun.sun_dir),environmentLight);
    if(material==0u&&(flags&APPLE_SUN_GLOSS)!=0u) {
        float3 n=normal*rsqrt(max(dot(normal,normal),1e-12));
        float3 delta=cameraPosition-position;
        float3 view=delta*rsqrt(max(dot(delta,delta),1e-12));
        float3 sum=view+sun.sun_dir;
        float3 halfVector=sum*rsqrt(max(dot(sum,sum),1e-12));
        float facing=saturate(dot(n,sun.sun_dir));
        float specular=pow(saturate(dot(n,halfVector)),24.0);
        if((flags&APPLE_LIGHT_BANDS)!=0u)specular=floor(specular*3.0+0.5)/3.0;
        float3 sunLight=sun.sun_color*sunLuminanceFromDirection(sun.sun_dir,sun.sun_luminance);
        color+=sunLight*sunTransmittance*specular*saturate(facing*4.0)*0.22;
    }
    return color;
}

public float3 shadeAppleSurface(U_GuiInput gui,U_SunInfo sun,U_ShadingInfo shading,
    U_ShadowCameraInfo shadowCamera,float3 position,float3 normal,float3 localPosition,
    uint material,float3 cameraPosition) {
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui,sun,shadowCamera,position,normal);
    float3 environment=sampleDiffuseEnvironment(gui,shading,position,normal);
    return applyAppleLighting(gui,sun,appleBaseColor(material,localPosition,gui.apple_appearance_flags),
        position,normal,material,cameraPosition,shadow.sun_transmittance,environment);
}
