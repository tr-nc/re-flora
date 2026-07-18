struct FragmentInput
{
    [[vk::location(0)]] float4 color_alpha : COLOR0;
};

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    float alpha = saturate(input.color_alpha.a);
    return float4(input.color_alpha.rgb * alpha, alpha);
}
