import model_cache_types;
export __extern_cpp int main() {
    // Includes atlas islands/boundaries and all values of each 12-bit axis.
    // The packed integer must stay exact and finite in the existing float slot.
    for(uint q=0u;q<4096u;q++) {
        float2 uv=float2(float(q)/4095.0,float(4095u-q)/4095.0);
        float encoded=encodeFlowerUV(uv);
        if(isnan(encoded)||isinf(encoded)||encoded<0.0||encoded>16777215.0)return 1;
        if(any(abs(decodeFlowerUV(encoded)-uv)>1e-7))return 2;
    }
    for(uint i=0u;i<256u;i++) {
        float2 uv=float2(float(i)/255.0,frac(float(i)*0.173));
        if(any(abs(decodeFlowerUV(encodeFlowerUV(uv))-uv)>0.5/4095.0+1e-7))return 3;
    }
    if(any(decodeFlowerUV(encodeFlowerUV(float2(-1,2)))!=float2(0,1)))return 4;
    return 0;
}
