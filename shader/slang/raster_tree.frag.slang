import tracer_types;
import color;
import contree_node;
import ddgi_query;
import terrain_ray_origin;
import terrain_edit_preview;
import tracer_material;
import voxel_types;
import tree_scene;
import tree_cells;
import raster_tree_shading;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(15,0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(48,0)]] StructuredBuffer<float4> raster_tree_light_cache;

float4 treeCachedLighting(float3 center) {
    uint3 p = uint3(floor(center*256.0));
    uint hash = p.x*73856093u ^ p.y*19349663u ^ p.z*83492791u;
    hash ^= hash>>16u;
    uint count,stride; raster_tree_cells.GetDimensions(count,stride);
    for (uint i=0u;i<64u;++i) {
        uint slot = (hash+i)&(count-1u);
        uint4 cell = raster_tree_cells[slot];
        if (cell.w==0u) break;
        if (all(cell.xyz==p)) return raster_tree_light_cache[slot];
    }
    return float4(0,0,0,1);
}
// Smooth value noise defines contiguous, irregular peeling areas rather than
// assigning unrelated pigments to rectangular cells. Rest-space sampling keeps
// the thin surface coloration attached to the same wood as it bends.
float barkNoise(float3 p) {
    int3 cell = int3(floor(p));
    float3 f = frac(p);
    f = f*f*(3.0-2.0*f);
    float value = 0.0;
    for (uint z=0u;z<2u;++z) for (uint y=0u;y<2u;++y) for (uint x=0u;x<2u;++x) {
        int3 c = cell + int3(x,y,z);
        uint h = uint(c.x)*73856093u ^ uint(c.y)*19349663u ^ uint(c.z)*83492791u;
        h ^= h >> 16u; h *= 0x7feb352du; h ^= h >> 15u;
        float3 w = lerp(1.0-f,f,float3(x,y,z));
        value += float(h & 65535u)/65535.0*w.x*w.y*w.z;
    }
    return value;
}
struct FragmentInput {
    [[vk::location(0)]] float3 world_position : TEXCOORD0;
    [[vk::location(1)]] nointerpolation float3 center : TEXCOORD1;
    [[vk::location(2)]] float3 normal : NORMAL0;
    [[vk::location(3)]] nointerpolation float bark_style : TEXCOORD2;
};
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0 {
    float3 receiverCenter = treeSceneActive() ? input.world_position : input.center;
    float4 cached = treeCachedLighting(input.center);
    float3 irradiance = cached.xyz;
    // Off and fully reliable cells retain the original shading. Fully degenerate
    // cells skip the smooth-normal DDGI query altogether; only the transition
    // band blends it with the once-per-voxel cached surface average.
    if (cached.w > 0.0) {
        float3 n = normalize(input.normal);
        float shadow = rasterTreeSunShadow(receiverCenter, n);
        float3 direct = rasterTreeSunIrradiance()
            * max(0.0,dot(normalize(sun_info.sun_dir),n)) * shadow;
        float3 ddgiPosition = terrainVoxelSurfacePositionAlongNormal(receiverCenter,n);
        float3 environment = sampleDdgiTerrainSmoothEnvironment(shading_info,ddgiPosition,input.world_position,n).irradiance;
        irradiance += cached.w * (environment + direct);
    }
    float3 albedo = voxelColorByType(voxel_colors,VOXEL_TYPE_CHERRY_WOOD);
    if(input.bark_style > 1.5) {
        // Gray-brown ginkgo bark: interrupted vertical ridges in rest space.
        // Pigment variation only; shared irradiance still owns all lighting.
        float3 p = input.center * 256.0;
        float ridges = barkNoise(p / float3(1.5,12.0,1.5));
        float3 pigment = lerp(float3(0.39,0.36,0.30),float3(0.55,0.51,0.43),
            smoothstep(0.25,0.72,ridges));
        albedo = srgbToLinear(pigment);
    } else if(input.bark_style > 0.5) {
        float3 p = input.center * 256.0;
        float3 warped = p + 1.1*sin(p.yzx*0.27 + sin(p.zxy*0.13));
        float patch = barkNoise(warped / float3(3.0,5.0,3.0));
        // Light brown outer bark and creamy olive exposed bark: close values,
        // not dark pseudo-shadows. Narrow transitions retain readable plates.
        float3 pigment = lerp(float3(0.65,0.63,0.53), float3(0.69,0.70,0.59),
            smoothstep(0.34,0.43,patch));
        pigment = lerp(pigment,float3(0.76,0.74,0.64),smoothstep(0.58,0.66,patch));
        albedo = srgbToLinear(pigment);
    }
    float3 color = albedo * irradiance;
    color = applyTerrainEditPreviewTint(terrain_edit_preview,color,receiverCenter);
    return float4(color,1.0);
}
