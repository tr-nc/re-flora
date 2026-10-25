module vegetation_water_sample;
import vegetation_water_types;
[[vk::binding(60,1)]] StructuredBuffer<uint4> vegetation_water_lookup;
[[vk::binding(61,1)]] StructuredBuffer<WaterResponseState> vegetation_water_states;
public WaterResponseState waterStateAt(uint index) {
    return index==0xffffffffu?(WaterResponseState)0:vegetation_water_states[index];
}
public WaterResponseState sampleVegetationWater(uint3 root,uint species) {
    uint4 header=vegetation_water_lookup[0];
    if(header.y==0u)return (WaterResponseState)0;
    uint4 key=uint4(root,species);
    uint mask=header.x, start=waterKeyHash(key)&mask;
    for(uint probe=0;probe<=mask;++probe) {
        uint slot=1+((start+probe)&mask)*2;
        uint4 stored=vegetation_water_lookup[slot];
        if(stored.w==0xffffffffu)break;
        if(all(stored==key))return waterStateAt(vegetation_water_lookup[slot+1].x);
    }
    return (WaterResponseState)0;
}
public float3 sampleLeafWaterByResponseIndex(uint index) {
    uint4 header=vegetation_water_lookup[0];
    if(header.y==0 || index>=header.w)return float3(0);
    return waterStateAt(vegetation_water_lookup[header.z+index].x).displacement_wetness.xyz;
}
