// Stage-one live preview. No atlas, interpolation, or persistent image cache.
module model_pixel_object;
import model_pixel_surface;
import model_pixel_projection;
import flora_shadow;
import environment_lighting;
import tracer_types;
[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(77,0)]] StructuredBuffer<float4> model_view_azimuths;
// Compute uses one read/write binding; graphics uses a separate NonWritable declaration.
[[vk::binding(6,1)]] public RWStructuredBuffer<float4> model_object_samples;
public ModelPixelView modelObjectView(uint index) {
    return modelPixelView(model_object_samples[index*4u+1u],model_object_samples[index*4u+2u],model_object_samples[index*4u+3u]);
}
// Production always uses object lighting. Zero is reserved for the continuous
// numerical oracle, which keeps its original per-texel reference shading.
public bool modelSingleLight() { return gui_input.model_pixel_view_count!=0u; }
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
            shadow_camera_info, pivot,float3(0,1,0));
        float3 ambient=(apple || gui_input.raster_flora_ddgi_lighting!=0u)
            ?sampleDiffuseEnvironment(gui_input,shading_info,pivot,float3(0,1,0))
            :LEGACY_RASTER_FLORA_AMBIENT_LIGHT*sun_info.sky_light_strength;
        light=float4(ambient,shadow.sun_transmittance);
    }
    model_object_samples[index*4u]=light;
    float3 delta=camera_info.pos.xyz-pivot;
    float3 view=length(delta)>1e-8?normalize(delta):orientation.axisZ;
    float3 chosen=modelDiscreteDirection(modelPixelLocalVector(orientation,view));
    // Fixed up rule for each view key, not camera-dependent framing.
    float3 up=abs(chosen.y)<0.99?float3(0,1,0):float3(1,0,0);
    float3 localX=normalize(cross(up,chosen)),localY=cross(chosen,localX);
    float3 x=orientation.axisX*localX.x+orientation.axisY*localX.y+orientation.axisZ*localX.z;
    float3 y=orientation.axisX*localY.x+orientation.axisY*localY.y+orientation.axisZ*localY.z;
    float3 z=orientation.axisX*chosen.x+orientation.axisY*chosen.y+orientation.axisZ*chosen.z;
    // Same shortest-arc correction drives roll and view selection. Transport to
    // the camera's forward plane before extracting roll (no off-axis skew).
    float3 right=rotateBetween(z,view,x);
    float3 cameraBack=normalize(mul(camera_info.view_mat_inv,float4(0,0,1,0)).xyz);
    right=rotateBetween(view,cameraBack,right);
    float2 roll=normalize(mul(camera_info.view_mat,float4(right,0)).xy);
    model_object_samples[index*4u+1u]=float4(x,roll.x);
    model_object_samples[index*4u+2u]=float4(y,roll.y);
    model_object_samples[index*4u+3u]=float4(z,0);
}
