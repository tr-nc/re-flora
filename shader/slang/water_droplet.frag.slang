struct FragmentInput
{
    [[vk::location(0)]] float4 color : COLOR0;
};

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    return float4(input.color.rgb * input.color.a, input.color.a);
}
