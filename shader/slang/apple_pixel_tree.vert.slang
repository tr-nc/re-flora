import apple_pixel_tree_pose;
import apple_pixel_frame;
import flora_vertex;
struct VertexInput {
    [[vk::location(0)]] uint corner:TEXCOORD0;
    uint instance_index:SV_VulkanInstanceID;
};
[shader("vertex")]
ApplePixelVertex main(VertexInput input) {
    ApplePixelVertex output=appleTreePose(input.instance_index);
    setApplePixelQuad(output,float2(input.corner&1u,(input.corner>>1u)&1u),camera_info);
    return output;
}
