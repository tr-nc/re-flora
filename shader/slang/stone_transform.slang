// The direct-stone integration seam: object-local bottom pivot + rigid pose.
// Direction snapping belongs to the view-quantization Worker, not this module.
module stone_transform;
public struct StonePreviewPush
{
    public float4 translation;
    public float4 rotation; // xyzw unit quaternion
    public float4 chunk_origin; // xyz local Contree origin, w side length
    public uint4 metadata; // node offset, leaf offset, material, reserved
};
public float3 stoneRotate(float3 p, float4 q)
{
    return p + 2.0 * cross(q.xyz, cross(q.xyz, p) + q.w * p);
}
public float3 stoneWorldPoint(float3 local, StonePreviewPush pose)
{
    return stoneRotate(local, pose.rotation) + pose.translation.xyz;
}
public float3 stoneLocalPoint(float3 world, StonePreviewPush pose)
{
    return stoneRotate(world - pose.translation.xyz,
                       float4(-pose.rotation.xyz, pose.rotation.w));
}
