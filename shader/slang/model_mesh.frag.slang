import model_mesh_types;
[shader("fragment")]
float4 main(ModelMeshOutput input):SV_Target { return input.color; }
