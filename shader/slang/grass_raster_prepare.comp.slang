import tracer_types;
import flora_types;
import grass_band_world;
import model_raster_grid;
import flower_stem_model_sampling;
import stem_proxy_projection;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::push_constant]] ConstantBuffer<PushConstantStemRaster> pass;
[[vk::binding(8, 1)]] RWStructuredBuffer<ModelRasterView> stem_raster_views;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= pass.instance_count) return;
    ModelRasterView view = {};
    stem_raster_views[id.x] = view;
    uint base = pass.cache_offset + (pass.first_instance + id.x) * pass.voxel_count;
    uint height = uint(grass_band_pose_cache[base].position_height.w);
    float3 lo = float3(1e20), hi = float3(-1e20);
    uint visible = 0u;
    for (uint band = 0u; band < min(height, pass.voxel_count); band++)
    {
        if (grass_band_pose_cache[base + band].color_valid.w == 0.0) continue;
        visible++;
        for (uint corner = 0u; corner < 8u; corner++)
        {
            float3 p = grassBandWorldPoint(base + band, band, height,
                pass.voxel_count, gui_input.stem_band_mode, corner);
            lo = min(lo, p); hi = max(hi, p);
        }
    }
    if (visible == 0u) return;
    float3 center = (lo + hi) * 0.5;
    float modelHeight = max(hi.y - lo.y, float(visible) / 256.0);
    uint resolution = max(gui_input.flower_stem_sampling.z, 1u);
    StemProjection lower = projectStemProxy(0u, camera_info, float3(0), 1.0,
        float2(1, 0), lo, hi, center, modelHeight, resolution, true);
    if (lower.position.z > 1.0) return;
    StemModelGrid grid = lower.grid;
    view.center_cell = float4(grid.center, grid.cell);
    view.y_size.w = -1.0; // Existing grid's singular/edge-on cases stay continuous.
    stem_raster_views[id.x] = view;
    if (grid.cell <= 1e-8) return;
    float distance = dot(grid.center - camera_info.pos.xyz, grid.normal);
    if (distance <= 1e-7) return;
    // Never spend multiple atlas texels on a subpixel display footprint. The
    // lattice remains model-anchored; this is a distance-dependent sampling LOD.
    float worldPixel = 2.0 * distance / max(abs(camera_info.proj_mat[1][1]) * float(pass.extent.y), 1.0);
    grid.cell = max(grid.cell, worldPixel * 0.5);
    float3 margin = float3(grid.cell * 2.0);
    lower = projectStemProxy(0u, camera_info, float3(0), 1.0, float2(1, 0),
        lo - margin, hi + margin, center, modelHeight, resolution, false);
    StemProjection upper = projectStemProxy(2u, camera_info, float3(0), 1.0, float2(1, 0),
        lo - margin, hi + margin, center, modelHeight, resolution, false);
    if (lower.position.z > 1.0) { view.y_size.w = 0.0; stem_raster_views[id.x] = view; return; }

    float2 uvLo = float2(1e20), uvHi = float2(-1e20);
    float minZ = 1e20;
    bool crossesEye = false;
    for (uint corner = 0u; corner < 8u; corner++)
    {
        float3 p = float3((corner & 1u) != 0u ? hi.x : lo.x,
            (corner & 2u) != 0u ? hi.y : lo.y, (corner & 4u) != 0u ? hi.z : lo.z);
        float z = dot(p - camera_info.pos.xyz, grid.normal);
        minZ = min(minZ, z);
        crossesEye = crossesEye || z <= 1e-7;
        if (z > 1e-7)
        {
            float2 uv = modelRasterPlaneUV(grid, camera_info.pos.xyz, p);
            uvLo = min(uvLo, uv); uvHi = max(uvHi, uv);
        }
    }
    if (crossesEye)
    {
        // The clipped screen rectangle bounds the visible sampling lattice even
        // when geometry crosses the eye; projecting only retained box corners
        // would miss arbitrarily large near-camera cells.
        uvLo = float2(1e20); uvHi = float2(-1e20);
        for (uint corner = 0u; corner < 4u; corner++)
        {
            float2 ndc = float2((corner & 1u) != 0u ? upper.position.x : lower.position.x,
                (corner & 2u) != 0u ? upper.position.y : lower.position.y);
            float4 rayV = mul(camera_info.proj_mat_inv, float4(ndc, 1, 1));
            float3 ray = mul(camera_info.view_mat_inv, float4(rayV.xyz, 0)).xyz;
            float facing = dot(ray, grid.normal);
            if (facing <= 1e-6) return;
            float2 uv = float2(dot(ray, grid.x), dot(ray, grid.y)) * distance / facing;
            uvLo = min(uvLo, uv); uvHi = max(uvHi, uv);
        }
    }
    // A bounded tile is a sampling LOD, never a plant-count truncation. All
    // instances are streamed through successive batches, without GPU readback.
    grid.cell = max(grid.cell, max(uvHi.x - uvLo.x, uvHi.y - uvLo.y) / float(pass.slot_side - 4u));
    float2 origin = floor(uvLo / grid.cell) - 1.0;
    float2 span = ceil(uvHi / grid.cell) - origin + 2.0;
    uint size = 8u;
    while (size < uint(max(span.x, span.y)) && size < pass.slot_side) size *= 2u;
    view.center_cell = float4(grid.center, grid.cell);
    view.x_near = float4(grid.x, max(1e-7, minZ > 1e-7 ? minZ * 0.5 : distance * 1e-5));
    view.y_size = float4(grid.y, float(size));
    view.normal_distance = float4(grid.normal, distance);
    view.cell_origin = float4(origin, 0, 0);
    // Include the footprint of the *effective* lattice, not its pre-LOD cell.
    margin = float3(grid.cell * 2.0);
    lower = projectStemProxy(0u, camera_info, float3(0), 1.0, float2(1, 0),
        lo - margin, hi + margin, center, modelHeight, resolution, false);
    upper = projectStemProxy(2u, camera_info, float3(0), 1.0, float2(1, 0),
        lo - margin, hi + margin, center, modelHeight, resolution, false);
    view.screen_bounds = float4(lower.position.xy, upper.position.xy);
    stem_raster_views[id.x] = view;
}
