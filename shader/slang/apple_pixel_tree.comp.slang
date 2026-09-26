import apple_pixel_tree_pose;
import apple_pixel_frame;
import apple_pixel_tile;
import model_pixel_object;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
import flora_vertex;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    ApplePixelVertex pose=appleTreePose(id.z);
    if(floraModelObjectPrepare()) {
        if(all(id.xy==uint2(0)))prepareModelObject(id.z,pose.center,appleModelFrame(pose),true);
        return;
    }
    if(any(id.xy>=uint2(pose.resolution)))return;
    model_pixel_tiles[(id.z*pose.resolution+id.y)*pose.resolution+id.x]=sampleAppleModel(pose,id.xy);
}
