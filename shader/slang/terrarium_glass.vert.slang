import tracer_types;

struct GlassPushConstants
{
    float4 box_min_near_alpha;
    float4 box_max_far_alpha;
};

[[vk::push_constant]]
ConstantBuffer<GlassPushConstants> pc;

[[vk::binding(3, 0)]]
ConstantBuffer<U_CameraInfo> camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position_ws : POSITION0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] float3 normal_ws : NORMAL0;
    [[vk::location(3)]] uint face_id : TEXCOORD1;
    [[vk::location(4)]] uint part_kind : TEXCOORD2;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 position_ws : TEXCOORD0;
    [[vk::location(1)]] float2 uv : TEXCOORD1;
    [[vk::location(2)]] nointerpolation uint face_id : TEXCOORD2;
    [[vk::location(3)]] nointerpolation float near_side : TEXCOORD3;
    [[vk::location(4)]] nointerpolation float2 alpha_pair : TEXCOORD4;
    [[vk::location(5)]] float3 normal_ws : NORMAL0;
    [[vk::location(6)]] nointerpolation uint part_kind : TEXCOORD5;
    [[vk::location(7)]] float3 view_direction_ws : TEXCOORD6;
};

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float3 boxMinimum = pc.box_min_near_alpha.xyz;
    float3 boxMaximum = pc.box_max_far_alpha.xyz;
    float3 boxCenter = (boxMinimum + boxMaximum) * 0.5;
    float3 cameraFromCenter = camera_info.pos.xyz - boxCenter;
    float3 normal = normalize(input.normal_ws);

    output.position_ws = input.position_ws;
    output.uv = input.uv;
    output.face_id = input.face_id;
    output.near_side = dot(normal, cameraFromCenter) >= 0.0 ? 1.0 : 0.0;
    output.alpha_pair = float2(
        pc.box_min_near_alpha.w, pc.box_max_far_alpha.w);
    output.normal_ws = normal;
    output.part_kind = input.part_kind;
    output.view_direction_ws = camera_info.pos.xyz - input.position_ws;
    output.position = mul(
        camera_info.view_proj_mat, float4(input.position_ws, 1.0));
    return output;
}
