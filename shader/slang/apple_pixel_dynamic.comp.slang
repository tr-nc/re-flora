import apple_pixel_dynamic_pose;
import apple_pixel_frame;
import apple_pixel_tile;
import tracer_types;
import model_pixel_object;
struct ApplePass { uint prepare; };
[[vk::push_constant]] ConstantBuffer<ApplePass> apple_pass;
// Same packed 44-byte stream used for the vertex attributes. Scalar loads avoid
// assuming a std430 float4 alignment that the vertex stream does not have.
[[vk::binding(4,1)]] StructuredBuffer<float> dynamic_fruit_pixel_instances;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    uint n=clamp(gui_input.apple_pixel_resolution,8u,64u);
    if(any(id.xy>=uint2(n)))return;
    uint b=id.z*14u;
    float3 center=float3(dynamic_fruit_pixel_instances[b],dynamic_fruit_pixel_instances[b+1],dynamic_fruit_pixel_instances[b+2]);
    float4 rotation=float4(dynamic_fruit_pixel_instances[b+7],dynamic_fruit_pixel_instances[b+8],dynamic_fruit_pixel_instances[b+9],dynamic_fruit_pixel_instances[b+10]);
    ApplePixelVertex pose=appleDynamicPose(center,rotation,dynamic_fruit_pixel_instances[b+6],n,id.z);
    if(apple_pass.prepare!=0u) {
        if(all(id.xy==uint2(0)))prepareModelObject(id.z,pose.center,appleModelFrame(pose),true);
        return;
    }
    model_pixel_tiles[(id.z*n+id.y)*n+id.x]=sampleAppleModel(pose,id.xy);
}
