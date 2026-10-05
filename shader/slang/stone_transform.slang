// The direct-stone integration seam: object-local bottom pivot + rigid pose.
// Direction snapping belongs to the view-quantization Worker, not this module.
module stone_transform;
import model_pixel_types;
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
// Canonical dimensions are baked into the mesh. One bottom-pivot rigid frame
// is shared by all vertices and normals; camera/style never changes the pose.
public ModelPixelFrame stonePhysicalFrame(StonePreviewPush pose)
{
    ModelPixelFrame frame = identityModelPixelFrame();
    frame.center = pose.translation.xyz;
    frame.axisX = stoneRotate(float3(1,0,0), pose.rotation);
    frame.axisY = stoneRotate(float3(0,1,0), pose.rotation);
    frame.axisZ = stoneRotate(float3(0,0,1), pose.rotation);
    return frame;
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
