// Attached and fallen leaves consume the identical model instances and display.
import butterfly_mesh_types;
import tracer_types;
import model_pixel_projection;
import model_pixel_view_data;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(11,1)]] StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
public struct LeafModelVertex {
    public float4 position:SV_Position;
    public [[vk::location(0)]] float2 uv:TEXCOORD0;
    public [[vk::location(1)]] nointerpolation uint instanceIndex:TEXCOORD1;
};
public LeafModelVertex displayAttachedLeaf(uint packed,uint index) {
    LeafModelVertex output;
    output.uv=float2((packed>>21u)&1u,(packed>>22u)&1u);
    output.instanceIndex=index;
    ButterflyMeshInstance instance=butterfly_mesh_instances[index];
    float4 bounds=butterflyTileBounds(instance,camera_info);
    output.position=gui_input.model_pixel_view_count!=0u
        ?modelOrthographicQuad(readModelPixelView(index),instance.position_size.xyz,
            instance.position_size.w*(1.53125*0.5),output.uv,camera_info)
        :float4(lerp(bounds.xy,bounds.zw,output.uv),0,1);
    if(instance.repair.w==0u)output.position=float4(2,2,2,1);
    return output;
}
