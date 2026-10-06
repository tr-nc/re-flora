// Plain-mesh consumer of GPU-prepared flower stem bands, like cached grass.
import stem_band_geometry;
import tracer_types;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(16,1)]] StructuredBuffer<StemMeshBand> flower_stem_bands;
struct VertexInput {
    [[vk::location(0)]] uint vertex:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
[shader("vertex")]
StemBandVertex main(VertexInput input) {
    StemBandVertex output;
    output.position=float4(2,2,2,1);output.color=float3(0);
    StemMeshBand band=flower_stem_bands[input.instance];
    if(band.color_valid.w==0.0)return output;
    output.position=mul(camera_info.view_proj_mat,float4(preparedStemBandPoint(band,input.vertex),1));
    output.color=band.color_valid.xyz;
    return output;
}
