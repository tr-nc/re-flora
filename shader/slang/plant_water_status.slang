// Reversible presentation derived from the simulation-owned plant water store.
// Pure helpers shared by grass and modeled flowers.
module plant_water_status;
import flora_types;
import voxel_types;
import hash;
import flora_noise;

public float plantStoredWaterDeficit(float hydration,float threshold) {
    float lack=saturate(1.0-hydration/max(threshold,0.001));
    return lack*lack;
}

// Root stays fixed; upper tissue bows sideways and downward. Stable per-root
// direction keeps dehydration distinct from wind and independent of draw order.
// Maximum vector length is < 0.25 * height, the existing flower motion budget.
public float3 plantWiltOffset(float deficit,float height,float fraction,uint seed,
                              float strength) {
    float angle=constructFloat01(wellonsHash(seed^0x86b27d49u))*6.28318530718;
    float t=saturate(fraction);
    // Presentation only: never modify root soil, growth, or the raw deficit.
    // Bound strength to preserve the existing shared displacement/culling budget.
    float amount=saturate(deficit)*saturate(strength)*max(height,0.0)*t*t;
    return float3(cos(angle)*0.18,-0.16,sin(angle)*0.18)*amount;
}
