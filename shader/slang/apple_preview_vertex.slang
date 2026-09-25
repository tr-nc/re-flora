module apple_preview_vertex;
import color;

public bool isApplePreviewVertex(uint packed) {return (packed & 0x80000000u)!=0u;}
public float3 applePreviewPosition(uint packed) {
    uint3 raw=uint3(packed & 511u,(packed>>9u)&511u,(packed>>18u)&511u);
    return (float3(raw)-128.0)/16.0;
}
public float3 applePreviewBaseColor(uint packed) {
    uint material=(packed>>27u)&3u;
    float3 srgb=material==0u ? float3(203.0,48.0,47.0)/255.0 :
        material==1u ? float3(116.0,80.0,46.0)/255.0 : float3(77.0,145.0,62.0)/255.0;
    return srgbToLinear(srgb);
}
