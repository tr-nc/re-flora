// Reflection-only ABI, following local_lighting_abi.slang. Not a runtime pass.
import sprinkler_mist_transport;
[[vk::binding(0,0)]] ConstantBuffer<B_SprinklerMistLobe> mist_lobe_abi;
[[vk::binding(1,0)]] ConstantBuffer<B_SprinklerMistBounds> mist_bounds_abi;
[[vk::binding(2,0)]] RWStructuredBuffer<float> mist_abi_output;
[shader("compute")]
[numthreads(1,1,1)]
void main() { mist_abi_output[0]=mist_lobe_abi.center_amplitude.w+mist_bounds_abi.lower.w; }
