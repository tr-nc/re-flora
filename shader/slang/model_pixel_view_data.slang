// Read-only graphics view of compute's per-object output. Keeping this separate
// avoids requiring vertex/fragment stores and gives the resource tracker one
// access declaration per buffer per dispatch/draw (no read/write descriptor alias).
module model_pixel_view_data;
import model_pixel_projection;
[[vk::binding(6,1)]] StructuredBuffer<float4> model_object_view_samples;
public ModelPixelView readModelPixelView(uint index) {
    return modelPixelView(model_object_view_samples[index*4u+1u],
        model_object_view_samples[index*4u+2u],model_object_view_samples[index*4u+3u]);
}
