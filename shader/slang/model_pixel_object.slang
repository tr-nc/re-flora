// Stage-one live preview. No atlas, interpolation, or persistent image cache.
module model_pixel_object;
import model_pixel_surface;
import flora_shadow;
import environment_lighting;
import tracer_types;
[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5,0)]] public Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(9,0)]] public Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10,0)]] public Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11,0)]] public Sampler2D cloud_shadow_tex;
[[vk::binding(77,0)]] StructuredBuffer<float4> model_view_azimuths;
// Four float4s per object: ambient + sunlight visibility; three rotation columns.
[[vk::binding(6,1)]] public RWStructuredBuffer<float4> model_object_samples;
public bool modelSingleLight() { return (gui_input.model_pixel_options&1u)!=0u; }
public bool modelDiscreteViews() { return gui_input.model_pixel_view_count!=0u; }
float3 rotateBetween(float3 a,float3 b,float3 v) {
    // Nearest of the distributed directions is never antipodal to the input.
    float3 c=cross(a,b);
    return v+cross(c,v)+cross(c,cross(c,v))/max(1.0+dot(a,b),1e-6);
}
public float3 modelDiscreteDirection(float3 view) {
    float best=-2.;float3 selected=float3(0,0,1);
    uint capacity,stride;model_view_azimuths.GetDimensions(capacity,stride);
    uint count=min(gui_input.model_pixel_view_count,capacity);
    for(uint i=0u;i<count;i++) {
        // Recompute latitudes for N: taking N entries from a larger sphere
        // would cluster directions into a cap instead of covering the sphere.
        float y=1.0-2.0*(float(i)+0.5)/float(count);
        float radius=sqrt(max(0.0,1.0-y*y));
        float2 azimuth=model_view_azimuths[i].xy;
        float3 direction=float3(azimuth.x*radius,y,azimuth.y*radius);
        float score=dot(view,direction);
        if(score>best){best=score;selected=direction;}
    }
    return selected;
}
public void prepareModelObject(uint index,float3 pivot,ModelPixelFrame orientation,bool apple) {
    float4 light=float4(0.);
    if(modelSingleLight()) {
        // One spatial lookup per object; no per-texel external-shadow queries.
        StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,
            shadow_camera_info,shadow_map_tex_for_vsm_ping,leaf_shadow_opacity_blended_tex,
            leaf_shadow_mask_tex,cloud_shadow_tex,pivot,float3(0,1,0));
        float3 ambient=(apple || gui_input.raster_flora_ddgi_lighting!=0u)
            ?sampleDiffuseEnvironment(gui_input,shading_info,pivot,float3(0,1,0))
            :LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
        light=float4(ambient,shadow.sun_transmittance);
    }
    model_object_samples[index*4u]=light;
    float3 x=float3(1,0,0),y=float3(0,1,0),z=float3(0,0,1);
    if(modelDiscreteViews()) {
        float3 delta=camera_info.pos.xyz-pivot;
        float3 view=length(delta)>1e-8?normalize(delta):orientation.axisZ;
        float3 local=modelPixelLocalVector(orientation,view);
        float3 chosen=modelDiscreteDirection(local);
        float3 from=orientation.axisX*chosen.x+orientation.axisY*chosen.y+orientation.axisZ*chosen.z;
        x=rotateBetween(from,view,x);y=rotateBetween(from,view,y);z=rotateBetween(from,view,z);
    }
    model_object_samples[index*4u+1u]=float4(x,0);
    model_object_samples[index*4u+2u]=float4(y,0);
    model_object_samples[index*4u+3u]=float4(z,0);
}
public ModelPixelFrame modelObjectFrame(uint index,ModelPixelFrame frame,float3 pivot) {
    if(!modelDiscreteViews())return frame;
    float3 x=model_object_samples[index*4u+1u].xyz;
    float3 y=model_object_samples[index*4u+2u].xyz;
    float3 z=model_object_samples[index*4u+3u].xyz;
    float3 c=frame.center-pivot;
    frame.center=pivot+x*c.x+y*c.y+z*c.z;
    frame.axisX=x*frame.axisX.x+y*frame.axisX.y+z*frame.axisX.z;
    frame.axisY=x*frame.axisY.x+y*frame.axisY.y+z*frame.axisY.z;
    frame.axisZ=x*frame.axisZ.x+y*frame.axisZ.y+z*frame.axisZ.z;
    return frame;
}
