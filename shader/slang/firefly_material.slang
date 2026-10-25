module firefly_material;
public float3 fireflyPigment(uint material) {
    if(material==1u)return float3(0.36,0.42,0.23);
    if(material==2u)return float3(0.38,0.52,0.04);
    return float3(0.08,0.11,0.025);
}
public float3 fireflyEmission(uint material,float pulse) {
    return material==2u?float3(0.70,1.0,0.08)*(2.6*pulse):float3(0);
}
