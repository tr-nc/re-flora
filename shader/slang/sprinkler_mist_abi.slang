// Reflection-only ABI, following local_lighting_abi.slang. Not a runtime pass.
import sprinkler_mist_transport;
import sprinkler_air_math;
[[vk::binding(4,0)]] ConstantBuffer<B_SprinklerAirInfo> air_info_abi;
import sprinkler_spray_contact;
[[vk::binding(3,0)]] ConstantBuffer<B_SprinklerWaterContact> water_contact_abi;
[[vk::binding(0,0)]] ConstantBuffer<B_SprinklerMistLobe> mist_lobe_abi;
[[vk::binding(1,0)]] ConstantBuffer<B_SprinklerMistBounds> mist_bounds_abi;
[[vk::binding(2,0)]] RWStructuredBuffer<float> mist_abi_output;
[shader("compute")]
[numthreads(1,1,1)]
void main() { mist_abi_output[0]=mist_lobe_abi.center_amplitude.w+mist_bounds_abi.lower.w+water_contact_abi.center_flux.w+air_info_abi.ambient_mass.w; }
