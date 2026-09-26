// Opt-in diagnostic consumer: exactly the production terrain DDGI query, with
// no local/direct lighting, albedo, exposure, or display transform added.
import ddgi_query;
import tracer_types;

public struct U_DdgiResponseRequest
{
    public uint4 identity;
    public float4 position;
    public float4 normal;
};

public struct B_DdgiResponseSample
{
    public uint4 identity;
    public float4 position;
    public float4 normal;
    public float4 irradiance_and_weight;
    public uint4 support;
};

ConstantBuffer<U_DdgiResponseRequest> ddgi_response_request;
ConstantBuffer<U_ShadingInfo> shading_info;
GLSLShaderStorageBuffer<B_DdgiResponseSample, Std430DataLayout> ddgi_response_sample;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    float3 position = ddgi_response_request.position.xyz;
    float3 normal = ddgi_response_request.normal.xyz;
    DdgiQueryResult result = sampleDdgiTerrainSmoothEnvironment(
        shading_info, position, position, normal);
    ddgi_response_sample.identity = uint4(ddgi_response_request.identity.x,
        shading_info.ddgi_ready, shading_info.ddgi_geometry_revision,
        shading_info.environment_revision);
    ddgi_response_sample.position = ddgi_response_request.position;
    ddgi_response_sample.normal = ddgi_response_request.normal;
    ddgi_response_sample.irradiance_and_weight = float4(result.irradiance, result.accumulated_weight);
    ddgi_response_sample.support = uint4(result.contributing_probe_count, result.dominant_probe_index, 0u, 0u);
}
