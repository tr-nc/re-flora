import firefly_material;
import butterfly_color;
public float3 gardenModelPigment(uint flags,uint material,float3 fallback) {
    if((flags&4u)!=0u)return fireflyPigment(material);
    if((flags&8u)!=0u)return material==1u?float3(.80,.88,.90):(material==2u?float3(.95,.65,.10):float3(.16,.10,.04));
    return fallback;
}
public float3 finishGardenModelColor(float3 color,float3 pigment,uint flags,uint material,float pulse,float sunAltitude) {
    if((flags&4u)!=0u)return color+fireflyEmission(material,pulse);
    return readableButterflyColor(color,pigment,sunAltitude);
}
