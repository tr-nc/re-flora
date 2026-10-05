struct Input
{
    [[vk::location(0)]] float4 color : COLOR0;
};
[shader("fragment")]
float4 main(Input input) : SV_Target0
{
    return input.color;
}
