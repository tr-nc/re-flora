module apple_pixel_tile;
import apple_pixel_frame;
import color;
import flora_shadow;
import model_pixel_types;
import tracer_types;

import model_pixel_object;
import model_pixel_projection;
import model_pixel_cache;
import model_cache_types;
import apple_appearance;

public ModelPixelFrame appleModelFrame(ApplePixelVertex input) {
    ModelPixelFrame frame;
    frame.center=input.center;frame.scale=input.scale;
    frame.axisX=input.axisX;frame.axisY=input.axisY;frame.axisZ=input.axisZ;
    return frame;
}
public float4 sampleAppleModel(ApplePixelVertex input,uint2 pixel) {
    ModelPixelFrame frame=appleModelFrame(input);
        uint view=uint(model_object_samples[input.tileIndex*4u+3u].w);
        ModelBakedSurface surface=modelSurface(APPLE_MODEL_SOURCE,view,input.resolution,pixel);
        if(surface.positionDepth.w>=1.0)return float4(0,0,0,1);
        float3 n=surface.normalMaterial.xyz;
        float3 normal=normalize(input.axisX*n.x+input.axisY*n.y+input.axisZ*n.z);
        float3 position=modelPixelWorld(frame,surface.positionDepth.xyz);
        if(gui_input.apple_appearance_flags==0u&&dot(normal,position-camera_info.pos.xyz)>0.0)normal=-normal;
        uint material=uint(surface.normalMaterial.w+0.5);
        float3 base=appleBaseColor(material,surface.positionDepth.xyz,gui_input.apple_appearance_flags);
        float3 color;
        if(gui_input.apple_appearance_flags==0u) {
            // Preserve this legacy consumer's original object-center lighting.
            float4 light=model_object_samples[input.tileIndex*4u];
            color=applyStylizedVoxelLightingWithEnvironment(sun_info,base,
                light.w*directionWeight(normal,sun_info.sun_dir),light.rgb);
        } else {
            color=shadeAppleSurface(gui_input,sun_info,shading_info,shadow_camera_info,
                position,normal,surface.positionDepth.xyz,material,camera_info.pos.xyz);
        }
        return float4(color,surface.positionDepth.w);
}
