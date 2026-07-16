import color;

struct Matrices
{
    float4x4 ortho;
};

[[vk::push_constant]]
ConstantBuffer<Matrices> matrices;

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] float4 color : COLOR0;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 color : COLOR0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
};

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    output.color = float4(srgbToLinear(input.color.rgb), input.color.a);
    output.uv = input.uv;
    output.position = mul(matrices.ortho, float4(input.position, 0.0, 1.0));
    return output;
}
