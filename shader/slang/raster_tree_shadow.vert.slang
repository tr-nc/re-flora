import tracer_types;
[[vk::binding(3,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
struct VertexInput {
    [[vk::location(0)]] float3 position : POSITION0;
};
struct VertexOutput { float4 position : SV_Position; };
[shader("vertex")]
VertexOutput main(VertexInput input) {
    VertexOutput output;
    output.position = mul(shadow_camera_info.view_proj_mat,float4(input.position,1));
    return output;
}
