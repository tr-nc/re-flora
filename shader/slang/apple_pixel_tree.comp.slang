import apple_pixel_tree_pose;
import apple_pixel_frame;
import apple_pixel_tile;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    ApplePixelVertex pose=appleTreePose(id.z);
    if(any(id.xy>=uint2(pose.resolution)))return;
    model_pixel_tiles[(id.z*pose.resolution+id.y)*pose.resolution+id.x]=sampleAppleModel(pose,id.xy);
}
