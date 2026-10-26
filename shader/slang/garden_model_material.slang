import firefly_material;
import butterfly_color;
public float3 gardenModelPigment(uint flags,uint material,float3 fallback) {
    if((flags&4u)!=0u)return fireflyPigment(material);
    if((flags&8u)!=0u)return material==1u?float3(.80,.88,.90):(material==2u?float3(.95,.65,.10):float3(.16,.10,.04));
    if((flags&16u)!=0u) {
        if(material==1u)return float3(.25,.14,.08);
        if(material==2u)return float3(.78,.62,.33);
        if(material==3u)return float3(.045,.03,.02);
        return float3(.59,.36,.15);
    }
    return fallback;
}
public float3 finishGardenModelColor(float3 color,float3 pigment,uint flags,uint material,float pulse,float sunAltitude) {
    if((flags&4u)!=0u)return color+fireflyEmission(material,pulse);
    if((flags&16u)!=0u)return color;
    return readableButterflyColor(color,pigment,sunAltitude);
}
