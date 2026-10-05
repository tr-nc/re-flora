import grass_raster_input;
import grass_band_world;
import grass_band_cache;
import model_raster_grid;
struct Input { [[vk::location(0)]] uint vertex : TEXCOORD0; uint instance : SV_VulkanInstanceID; };
[shader("vertex")]
ModelRasterTileVertex main(Input input)
{
    ModelRasterTileVertex output = {}; output.position = float4(2, 2, 2, 1);
    uint object = rasterGrassObject(input.instance);
    ModelRasterView view = stem_raster_views[object];
    if (view.y_size.w <= 0.0) return output;
    uint band = input.vertex >> 3u;
    if (band >= raster_pass.voxel_count) return output;
    uint base = raster_pass.cache_offset + (raster_pass.first_instance + object) * raster_pass.voxel_count;
    GrassBandPose pose = grass_band_pose_cache[base + band];
    uint height = uint(pose.position_height.w);
    if (band >= height || pose.color_valid.w == 0.0) return output;
    float3 world = grassBandWorldPoint(base + band, band, height,
        raster_pass.voxel_count, input.vertex);
    output.position = modelRasterClip(view, camera_info.pos.xyz, world, object, raster_pass.slot_side);
    output.color = pose.color_valid.xyz;
    output.eye_depth = dot(world - camera_info.pos.xyz, view.normal_distance.xyz);
    output.tile_rect = float4(float2(modelRasterTileOrigin(object, raster_pass.slot_side)), float2(view.y_size.w));
    return output;
}
