import model_raster_grid;
struct TileFragment { float4 color : SV_Target0; float depth : SV_Depth; };
[shader("fragment")]
TileFragment main(ModelRasterTileVertex input)
{
    // A transparent far-depth result fails LESS against cleared active cells.
    // Avoid discard: the Vulkan device does not enable the demote capability.
    TileFragment output; output.color = float4(0); output.depth = 1.0;
    float2 pixel = floor(input.position.xy);
    if (any(pixel < input.tile_rect.xy) || any(pixel >= input.tile_rect.xy + input.tile_rect.zw)) return output;
    output.color = float4(input.color, input.eye_depth);
    output.depth = input.position.z;
    return output;
}
