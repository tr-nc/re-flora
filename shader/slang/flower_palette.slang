// Same sRGB8 palette-weight texture as the web: texels are decoded to linear
// before bilinear filtering, like Three's sRGB DataTexture (flipY=false).
module flower_palette;
[[vk::binding(18,1)]] public StructuredBuffer<float4> model_cache_palette;
import model_cache_types;
public float3 flowerPaletteColor(uint species,float encodedUV) {
    uint base=species*4u;
    uint offset=uint(model_cache_palette[base].w);
    int2 size=int2(model_cache_palette[base+1u].w,model_cache_palette[base+2u].w);
    float2 pixel=decodeFlowerUV(encodedUV)*float2(size)-0.5;
    int2 lo=int2(floor(pixel));float2 t=frac(pixel);
    int2 a=clamp(lo,int2(0),size-1),b=clamp(lo+1,int2(0),size-1);
    float3 c00=model_cache_palette[offset+uint(a.y*size.x+a.x)].xyz;
    float3 c10=model_cache_palette[offset+uint(a.y*size.x+b.x)].xyz;
    float3 c01=model_cache_palette[offset+uint(b.y*size.x+a.x)].xyz;
    float3 c11=model_cache_palette[offset+uint(b.y*size.x+b.x)].xyz;
    return lerp(lerp(c00,c10,t.x),lerp(c01,c11,t.x),t.y);
}
