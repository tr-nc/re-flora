// Same egui vertex layout/projection; only material meshes use this pipeline.
struct Matrices { float4x4 ortho; };
[[vk::push_constant]] ConstantBuffer<Matrices> matrices;

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] float4 color : COLOR0;
};
struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float opacity : COLOR0;
    [[vk::location(1)]] float2 local_points : TEXCOORD0;
    [[vk::location(2)]] float2 screen_uv : TEXCOORD1;
    [[vk::location(3)]] float2 point_uv : TEXCOORD2;
};

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    output.position = mul(matrices.ortho, float4(input.position, 0.0, 1.0));
    output.opacity = input.color.a;
    output.local_points = input.uv;
    output.screen_uv = output.position.xy * 0.5 + 0.5;
    output.point_uv = float2(matrices.ortho[0][0], matrices.ortho[1][1]) * 0.5;
    return output;
}
