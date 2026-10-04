module grass_raster_input;
import tracer_types;
import flora_types;
import model_raster_grid;

[[vk::binding(0, 0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::push_constant]] public ConstantBuffer<PushConstantStemRaster> raster_pass;
[[vk::binding(8, 1)]] public StructuredBuffer<ModelRasterView> stem_raster_views;

public uint rasterGrassObject(uint instance)
{
    return instance - raster_pass.species * MAX_FLORA_INSTANCES_PER_SPECIES - raster_pass.first_instance;
}
public float2 rasterQuadCorner(uint vertex)
{
    uint corners[6] = { 0u, 1u, 2u, 0u, 2u, 3u };
    uint i = corners[vertex];
    return float2(i == 1u || i == 2u ? 1.0 : 0.0, i >= 2u ? 1.0 : 0.0);
}
