// Native-triangle adapter; shared by flowers, apples, particles and future stones.
module model_mesh_view;
import model_view_quantization;
import model_pixel_types;
import model_mesh_lighting;

// Immutable golden-angle azimuths generated once by model_pixel_views::azimuths.
// Binding 19 avoids native mesh ranges (17) and flower material palette (18).
[[vk::binding(19,1)]] public StructuredBuffer<float4> model_view_azimuths;
struct NativeModelViewBank : IModelViewBank {
    float3 direction(uint index, uint count) {
        return modelViewDirection(model_view_azimuths[index].xy, index, count);
    }
}
public ModelPixelFrame modelMeshViewFrame(ModelPixelFrame physical, float3 localPivot) {
    NativeModelViewBank bank;
    return quantizeModelView(physical, localPivot, camera_info.pos.xyz,
        gui_input.model_pixel_view_count, bank);
}
