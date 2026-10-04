import grass_raster_input;
import model_raster_grid;
[shader("vertex")]
ModelRasterDisplayVertex main(uint vertex : SV_VulkanVertexID, uint instance : SV_VulkanInstanceID)
{
    ModelRasterDisplayVertex output = {}; output.position = float4(2, 2, 2, 1);
    uint object = rasterGrassObject(instance);
    ModelRasterView view = stem_raster_views[object];
    if (view.y_size.w <= 0.0) return output;
    float2 ndc = lerp(view.screen_bounds.xy, view.screen_bounds.zw, rasterQuadCorner(vertex));
    output.position = float4(ndc, 0, 1);
    float4 rayV = mul(camera_info.proj_mat_inv, float4(ndc, 1, 1));
    output.ray = mul(camera_info.view_mat_inv, float4(rayV.xyz, 0)).xyz;
    output.object = object;
    return output;
}
