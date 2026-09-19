import tracer_types;
import tree_surface;
[[vk::binding(3,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
struct VertexOutput { float4 position : SV_Position; };
[shader("vertex")]
VertexOutput main(uint vertex : SV_VulkanVertexID) {
    VertexOutput output;
    float3 position = treeSurfacePosition(vertex);
    output.position = mul(shadow_camera_info.view_proj_mat,float4(position,1));
    return output;
}
