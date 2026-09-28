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
    uint layer=input.vertex/36u;
    if(layer>=pose.layers) {
        output.position=float4(2,2,2,1);output.world=float3(0);output.normal=float3(0,1,0);output.color=float3(0);
        return output;
    }
    float sourceT=float(layer)/max(part.stem.w/12.0-1.0,1.0);
    float plantT=float(layer)/max(float(pose.layers)-1.0,1.0);
    local.x+=pose.bend*plantT*plantT*(3.0-2.0*plantT)-part.stem.x*sourceT*sourceT*(3.0-2.0*sourceT);
    output.world=flowerStemWorld(pose,local,tri.normal0.w);
    output.position=mul(camera_info.view_proj_mat,float4(output.world,1));
    if(frame.scale<=1e-7)output.position=float4(2,2,2,1);
    output.normal=flowerNormal(frame,tri.normal0.xyz);
    // Color is game-owned and evaluated along the undeformed stalk. Wind and
    // overall scale cannot move the gradient; the pixelated calyx is unaffected.
    output.color=flowerStemColor(local.y,pose.height);
    return output;
}
