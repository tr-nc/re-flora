module wind_field;

// One persistent horizontal transport field, shared by every wind input.
public struct U_WindFieldInfo {
    public float4 domain;
    public float4 cells[512];
};
[[vk::binding(4, 0)]] ConstantBuffer<U_WindFieldInfo> wind_field_info;

public bool authoredWindEnabled() { return wind_field_info.domain.w > 0.5; }

float2 fieldCell(uint2 p) {
    uint index = p.y * 32u + p.x;
    float4 pair = wind_field_info.cells[index / 2u];
    return (index & 1u) == 0u ? pair.xy : pair.zw;
}

public float2 sampleAuthoredWind(float2 worldVoxels) {
    float2 q = clamp(worldVoxels / wind_field_info.domain.xy * 31.0, 0.0, 31.0);
    uint2 base = min(uint2(q), uint2(30u));
    float2 f = q - float2(base);
    return lerp(lerp(fieldCell(base), fieldCell(base + uint2(1, 0)), f.x),
                lerp(fieldCell(base + uint2(0, 1)), fieldCell(base + uint2(1, 1)), f.x), f.y);
}
