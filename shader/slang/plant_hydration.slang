// Read-only simulation state, parallel to the unchanged eight-byte draw stream.
module plant_hydration;
import flora_types;
import plant_water_status;
[[vk::binding(32,1)]] StructuredBuffer<float> plant_hydration_values;
public float plantHydrationGrowthFactor(U_FloraGrowthInfo growth,uint species,uint index) {
    float level=saturate(plant_hydration_values[floraSpeciesInstanceOffset(species)+index])*3.0;
    uint lower=uint(floor(level));
    uint upper=min(lower+1u,3u);
    return lerp(growth.moisture_growth_factors[species][lower],
                growth.moisture_growth_factors[species][upper],frac(level));
}
public float plantHydrationDeficit(uint species,uint index,float threshold) {
    float h=plant_hydration_values[floraSpeciesInstanceOffset(species)+index];
    return plantStoredWaterDeficit(h,threshold);
}
