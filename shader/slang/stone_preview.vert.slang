import stone_transform;
import stone_lighting;
struct StoneTriangle
{
    float4 a;
    float4 e1;
    float4 e2;
    float4 normal;
    float4 color; // source sRGB attributes retained; native palette is material-owned
};
[[vk::binding(0,1)]] StructuredBuffer<StoneTriangle> stone_triangles;
[[vk::push_constant]] ConstantBuffer<StonePreviewPush> stone_pose;
struct Output
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 color : COLOR0;
};
[shader("vertex")]
// Native zero-based Vulkan draw index; HLSL SV_VertexID would add BaseVertex
// and an unnecessary optional DrawParameters capability on this device path.
Output main(uint vertex : SV_VulkanVertexID)
{
    StoneTriangle t = stone_triangles[vertex / 3u];
    uint corner = vertex % 3u;
    float3 local = t.a.xyz + (corner == 1u ? t.e1.xyz : corner == 2u ? t.e2.xyz : float3(0));
    // Keep this one local->world seam for the controller's direction helper.
    float3 position = stoneWorldPoint(local, stone_pose);
    float3 normal = normalize(stoneRotate(t.normal.xyz, stone_pose.rotation));
    Output output;
    output.position = mul(camera_info.view_proj_mat, float4(position,1));
    output.color = float4(stoneShade(stone_pose.metadata.z,position,normal),1);
    return output;
}
