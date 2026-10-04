import grass_raster_input;
import model_raster_grid;
[[vk::binding(7, 1)]] Sampler2D<float4> stem_raster_color;
struct DisplayFragment { float4 color : SV_Target0; float depth : SV_Depth; };
[shader("fragment")]
DisplayFragment main(ModelRasterDisplayVertex input)
{
    DisplayFragment output; output.color = float4(0); output.depth = 1.0;
    ModelRasterView view = stem_raster_views[input.object];
    float3 ray = normalize(input.ray);
    if (dot(ray, view.normal_distance.xyz) <= 1e-6) return output;
    int2 cell = int2(modelRasterSampleCell(view, camera_info.pos.xyz, ray));
    if (any(cell < int2(0)) || any(cell >= int2(view.y_size.w))) return output;
    int2 pixel = int2(modelRasterTileOrigin(input.object, raster_pass.slot_side)) + cell;
    float4 sample = stem_raster_color.SampleLevel((float2(pixel) + 0.5) / float(MODEL_RASTER_ATLAS_SIDE), 0.0);
    if (sample.w <= 0.0) return output;
    float3 world = modelRasterDisplayPoint(view, camera_info.pos.xyz, ray, sample.w);
    float4 clip = mul(camera_info.view_proj_mat, float4(world, 1));
    float depth = clip.z / clip.w;
    if (clip.w <= 0.0 || depth < 0.0 || depth > 1.0) return output;
    output.color = float4(sample.xyz, 1); output.depth = depth;
    return output;
}
