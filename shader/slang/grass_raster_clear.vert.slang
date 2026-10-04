import grass_raster_input;
import model_raster_grid;
[shader("vertex")]
float4 main(uint vertex : SV_VulkanVertexID, uint instance : SV_VulkanInstanceID) : SV_Position
{
    uint object = rasterGrassObject(instance);
    ModelRasterView view = stem_raster_views[object];
    if (view.y_size.w <= 0.0) return float4(2, 2, 2, 1);
    float2 pixel = float2(modelRasterTileOrigin(object, raster_pass.slot_side))
        + rasterQuadCorner(vertex) * view.y_size.w;
    return float4(pixel * (2.0 / float(MODEL_RASTER_ATLAS_SIDE)) - 1.0, 0, 1);
}
