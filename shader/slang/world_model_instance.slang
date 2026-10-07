// Frame-published world transform, shared by visible and sun-shadow draws.
// There is no editor tint, screen-space placement, or independent shadow pose.
module world_model_instance;
import model_mesh_view;
import model_pixel_types;

public struct WorldModelInstance {
    public float4 position;
    public float4 axisX;
    public float4 axisY;
    public float4 axisZ;
    public uint4 metadata;
    public float4 mouth;
};
[[vk::binding(0,1)]] public StructuredBuffer<WorldModelInstance> world_model_instances;

public ModelPixelFrame worldModelPhysicalFrame(WorldModelInstance instance) {
    ModelPixelFrame frame;
    frame.center=instance.position.xyz;
    frame.scale=instance.position.w;
    frame.axisX=instance.axisX.xyz;
    frame.axisY=instance.axisY.xyz;
    frame.axisZ=instance.axisZ.xyz;
    return frame;
}
public ModelPixelFrame worldModelRenderedFrame(WorldModelInstance instance) {
    return modelMeshViewFrame(worldModelPhysicalFrame(instance),instance.mouth.xyz);
}
