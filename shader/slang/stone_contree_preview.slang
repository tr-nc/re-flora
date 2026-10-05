// Real model->voxel->surface->Contree primary visibility, on the unchanged
// scene-wide grid. Private model buffers; no edits to the garden's world atlas.
import stone_transform;
import stone_lighting;
import contree_marching;
import contree_node;
import voxel_data;
import ray;
import packer;
[[vk::binding(12,0)]] [[vk::image_format("r32ui")]] RWTexture2D<uint> compute_output_tex;
[[vk::binding(13,0)]] [[vk::image_format("r32f")]] RWTexture2D<float> compute_depth_tex;
[[vk::binding(0,1)]] StructuredBuffer<ContreeNode> stone_nodes;
[[vk::binding(1,1)]] StructuredBuffer<uint> stone_leaves;
[[vk::push_constant]] ConstantBuffer<StonePreviewPush> stone_pose;
[shader("compute")]
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID, uint invocation : SV_GroupIndex)
{
    uint width,height;
    compute_output_tex.GetDimensions(width,height);
    if (any(id.xy >= uint2(width,height))) return;
    Ray ray = generateCameraRay((float2(id.xy)+0.5)/float2(width,height),camera_info.view_proj_mat_inv);
    float4 inverseRotation = float4(-stone_pose.rotation.xyz,stone_pose.rotation.w);
    float3 origin = stoneLocalPoint(ray.origin,stone_pose);
    float3 direction = stoneRotate(ray.direction,inverseRotation);
    ContreeMarchingResult hit = marchContree(stone_nodes,origin,direction,
        stone_pose.chunk_origin.xyz,float3(stone_pose.chunk_origin.w),false,
        stone_pose.metadata.x,stone_pose.metadata.y,invocation);
    if (!hit.is_hit) return;
    float3 world = stoneWorldPoint(hit.position,stone_pose);
    float4 projected = mul(camera_info.view_proj_mat,float4(world,1));
    float depth = projected.z/projected.w;
    if (projected.w <= 0 || depth < 0 || depth >= compute_depth_tex[id.xy]) return;
    uint data = stone_leaves[hit.voxel_address];
    float3 normal = normalize(stoneRotate(voxelSurfaceNormalFromData(data),stone_pose.rotation));
    compute_output_tex[id.xy] = packRgbe(stoneShade(voxelTypeFromData(data),world,normal));
    compute_depth_tex[id.xy] = depth;
}
