module wind_volume_sample;
import wind_field;

public float3 sampleWindField(float3 worldPosition) {
    float2 wind = authoredWindEnabled() ? sampleAuthoredWind(worldPosition.xz * 256.0) : float2(0.0);
    return float3(wind.x, 0.0, wind.y);
}
