import cpu_stem_band_input;
import tracer_types;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[shader("vertex")]
float4 main(CpuStemBandInput input):SV_Position {
    return mul(shadow_camera_info.view_proj_mat,float4(cpuStemBandPosition(input),1));
}
