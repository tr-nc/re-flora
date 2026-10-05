module model_mesh_types;
public struct ModelMeshOutput {
    public float4 position : SV_Position;
    public [[vk::location(0)]] float4 color : COLOR0;
};
