import tree_pixel_pass;
[[vk::binding(0,1)]] StructuredBuffer<float4> tree_pixel_tiles;
[[vk::push_constant]] TreePixelPass pass;
struct PixelOutput { float4 color:SV_Target0; float depth:SV_Depth; };
[shader("fragment")]
PixelOutput main(float4 position:SV_Position) {
    uint2 pixel=uint2(position.xy)/pass.view.z;
    uint2 grid=treePixelGrid(pass);
    // Same transparent/far-depth convention as the common model display; no
    // fragment-demote capability is required by the device contract.
    PixelOutput result;result.color=float4(0);result.depth=1.0;
    if(pixel.y<pass.band.x || pixel.y>=pass.band.x+pass.band.y)return result;
    float4 sample=tree_pixel_tiles[(pixel.y-pass.band.x)*grid.x+pixel.x];
    if(sample.w<1.0){result.color=float4(sample.xyz,1);result.depth=sample.w;}
    return result;
}
