import grass_raster_input;
import grass_band_world;
import grass_band_cache;
import model_raster_grid;
import stem_band_geometry;
struct Input { [[vk::location(0)]] uint vertex : TEXCOORD0; uint instance : SV_VulkanInstanceID; };
[shader("vertex")]
StemBandVertex main(Input input)
{
    StemBandVertex output = {}; output.position = float4(2, 2, 2, 1);
    uint object = rasterGrassObject(input.instance);
    if (stem_raster_views[object].y_size.w >= 0.0) return output;
    uint band = input.vertex >> 3u;
    if (band >= raster_pass.voxel_count) return output;
    uint base = raster_pass.cache_offset + (raster_pass.first_instance + object) * raster_pass.voxel_count;
    GrassBandPose pose = grass_band_pose_cache[base + band];
    uint height = uint(pose.position_height.w);
    if (band >= height || pose.color_valid.w == 0.0) return output;
    float3 world = grassBandWorldPoint(base + band, band, height,
        raster_pass.voxel_count, input.vertex);
    output.position = mul(camera_info.view_proj_mat, float4(world, 1));
    output.color = pose.color_valid.xyz;
    return output;
}
