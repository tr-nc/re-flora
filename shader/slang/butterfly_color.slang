// Thin, colored paper-like wings remain legible while resting or back-facing.
// This is a daylight-only stylized fill, not emission or a new light source.
module butterfly_color;
public float3 readableButterflyColor(float3 shaded,float3 pigment,float sunAltitude) {
    float daylight=smoothstep(-0.10,0.15,sunAltitude);
    float chroma=max(pigment.r,max(pigment.g,pigment.b))-min(pigment.r,min(pigment.g,pigment.b));
    float colored=saturate(chroma*4.0);
    return max(shaded*(1.0+0.25*daylight*colored),pigment*(0.22*daylight));
}
