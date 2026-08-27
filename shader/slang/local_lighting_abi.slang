import local_lighting;

[[vk::binding(0, 0)]] ConstantBuffer<U_LightGpu> local_light_abi;
[[vk::binding(1, 0)]] ConstantBuffer<U_LocalLightInfo> local_light_info_abi;
[[vk::binding(2, 0)]] RWStructuredBuffer<uint> local_light_abi_output;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    local_light_abi_output[0] = local_light_abi.abi_version +
        local_light_info_abi.abi_version;
}
