import flower_model;
import model_pixel_types;
import model_bake_triangle;
[[vk::binding(16,1)]] StructuredBuffer<ButterflyMeshTriangle> flower_triangles;
import tracer_types;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct VertexInput {
    [[vk::location(0)]] uint vertex:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
struct StemVertex {
    float4 position:SV_Position;
    [[vk::location(0)]] float3 world:TEXCOORD0;
    [[vk::location(1)]] float3 normal:TEXCOORD1;
    [[vk::location(2)]] float3 color:COLOR0;
};
[shader("vertex")]
StemVertex main(VertexInput input) {
    FlowerModelPart part=flowerPart(0u);
    ButterflyMeshTriangle tri=flower_triangles[part.range.x+input.vertex/3u];
    uint corner=input.vertex%3u;
    float3 local=tri.a.xyz+(corner==1u?tri.e1.xyz:corner==2u?tri.e2.xyz:float3(0));
    FlowerPlantPose pose=flowerPlantPose(input.instance);
    ModelPixelFrame frame=pose.frame;
    StemVertex output;
    output.world=flowerStemWorld(pose,local,tri.normal0.w);
    output.position=mul(camera_info.view_proj_mat,float4(output.world,1));
    if(frame.scale<=1e-7)output.position=float4(2,2,2,1);
    output.normal=flowerNormal(frame,tri.normal0.xyz);
    output.color=tri.uv01.xyz;
    return output;
}
