// Shared model-anchored perspective lattice for raster-generated color/depth tiles.
module model_raster_grid;
import flower_stem_model_sampling;

public static const uint MODEL_RASTER_ATLAS_SIDE = 2048u;
public struct PushConstantStemRaster
{
    public uint first_instance;
    public uint instance_count;
    public uint species;
    public uint cache_offset;
    public uint voxel_count;
    public uint slot_side;
    public uint2 extent;
};
public struct ModelRasterTileVertex
{
    public float4 position : SV_Position;
    [[vk::location(0)]] public nointerpolation float3 color : COLOR0;
    [[vk::location(1)]] public float eye_depth : TEXCOORD0;
    [[vk::location(2)]] public nointerpolation float4 tile_rect : TEXCOORD1;
};
public struct ModelRasterDisplayVertex
{
    public float4 position : SV_Position;
    [[vk::location(0)]] public noperspective float3 ray : TEXCOORD0;
    [[vk::location(1)]] public nointerpolation uint object : TEXCOORD1;
};
public struct ModelRasterView
{
    public float4 center_cell;
    public float4 x_near;
    public float4 y_size; // w: active tile side; 0 culled; -1 continuous-grid fallback
    public float4 normal_distance;
    public float4 cell_origin;
    public float4 screen_bounds;
};
public StemModelGrid modelRasterGrid(ModelRasterView view)
{
    StemModelGrid grid;
    grid.center = view.center_cell.xyz;
    grid.cell = view.center_cell.w;
    grid.x = view.x_near.xyz;
    grid.y = view.y_size.xyz;
    grid.normal = view.normal_distance.xyz;
    return grid;
}
public uint2 modelRasterTileOrigin(uint object, uint slotSide)
{
    uint columns = MODEL_RASTER_ATLAS_SIDE / slotSide;
    return uint2(object % columns, object / columns) * slotSide;
}
public float2 modelRasterPlaneUV(StemModelGrid grid, float3 camera, float3 world)
{
    float3 delta = world - camera;
    float distance = dot(grid.center - camera, grid.normal);
    float facing = dot(delta, grid.normal);
    return float2(dot(delta, grid.x), dot(delta, grid.y)) * distance / facing;
}
public float4 modelRasterClip(ModelRasterView view, float3 camera, float3 world,
    uint object, uint slotSide)
{
    StemModelGrid grid = modelRasterGrid(view);
    float z = dot(world - camera, grid.normal);
    float3 delta = world - camera;
    float2 projected = float2(dot(delta, grid.x), dot(delta, grid.y))
        * view.normal_distance.w / grid.cell;
    float2 offset = float2(modelRasterTileOrigin(object, slotSide)) - view.cell_origin.xy + 0.5;
    // Homogeneous projection also handles triangles crossing z=0: no divide by
    // zero before the hardware clipper intersects their edges with the near plane.
    float2 clipXY = (projected + offset * z) * (2.0 / float(MODEL_RASTER_ATLAS_SIDE)) - z;
    return float4(clipXY, z - view.x_near.w, z);
}
public float3 modelRasterDisplayPoint(ModelRasterView view, float3 camera,
    float3 displayRay, float eyeDepth)
{
    // The atlas stores eye depth as full float32 color data, independent of the
    // hardware depth test used to resolve mesh visibility inside the private tile.
    return camera + displayRay * (eyeDepth / dot(displayRay, view.normal_distance.xyz));
}
public float2 modelRasterSampleCell(ModelRasterView view, float3 camera, float3 displayRay)
{
    StemModelGrid grid = modelRasterGrid(view);
    float planeDistance = dot(grid.center - camera, grid.normal) / dot(displayRay, grid.normal);
    float3 point = camera + displayRay * planeDistance - grid.center;
    float2 uv = float2(dot(point, grid.x), dot(point, grid.y));
    return floor(uv / grid.cell + 0.5) - view.cell_origin.xy;
}
