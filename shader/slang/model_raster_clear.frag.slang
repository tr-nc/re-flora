struct TileClear { float4 color : SV_Target0; float depth : SV_Depth; };
[shader("fragment")]
TileClear main()
{
    TileClear output; output.color = float4(0); output.depth = 1.0;
    return output;
}
