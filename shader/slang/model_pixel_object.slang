// Per-object lighting/pose stays dynamic; canonical surfaces are shared.
module model_pixel_object;
import model_pixel_types;
import model_pixel_projection;
import flora_shadow;
import environment_lighting;
import tracer_types;
[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
import model_pixel_views;
import model_pixel_cache;
// Compute uses one read/write binding; graphics uses a separate NonWritable declaration.
[[vk::binding(6,1)]] public RWStructuredBuffer<float4> model_object_samples;
public ModelPixelView modelObjectView(uint index) {
    return modelPixelView(model_object_samples[index*4u+1u],model_object_samples[index*4u+2u],model_object_samples[index*4u+3u]);
}
public void prepareModelObject(uint index,float3 pivot,ModelPixelFrame orientation,bool apple,uint source) {
    float4 light=float4(0.);
    {
        // One spatial lookup per object; no per-texel external-shadow queries.
        StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,
            shadow_camera_info, pivot,float3(0,1,0));
        float3 ambient=(apple || gui_input.raster_flora_ddgi_lighting!=0u)
            ?sampleDiffuseEnvironment(gui_input,shading_info,pivot,float3(0,1,0))
            :LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
        light=float4(ambient,shadow.sun_transmittance);
    }
    model_object_samples[index*4u]=light;
    float3 delta=camera_info.pos.xyz-pivot;
    float3 view=length(delta)>1e-8?normalize(delta):orientation.axisZ;
    uint views=modelSourceViewCount(source);
    uint viewIndex=nearestModelView(modelPixelLocalVector(orientation,view),views);
    ModelPixelView canonical=canonicalModelView(viewIndex,views);
    float3 chosen=canonical.z,localX=canonical.x,localY=canonical.y;
    float3 x=orientation.axisX*localX.x+orientation.axisY*localX.y+orientation.axisZ*localX.z;
    float3 y=orientation.axisX*localY.x+orientation.axisY*localY.y+orientation.axisZ*localY.z;
    float3 z=orientation.axisX*chosen.x+orientation.axisY*chosen.y+orientation.axisZ*chosen.z;
    // Same shortest-arc correction drives roll and view selection. Transport to
    // the camera's forward plane before extracting roll (no off-axis skew).
    float2 roll=modelObjectRoll(x,z,pivot,camera_info);
    model_object_samples[index*4u+1u]=float4(x,roll.x);
    model_object_samples[index*4u+2u]=float4(y,roll.y);
    model_object_samples[index*4u+3u]=float4(z,float(viewIndex));
}
