import grass_growth_shape;

export __extern_cpp int main()
{
    float3 bottom=float3(0.1,0.5,0.2),tip=float3(0.8,0.9,0.1);
    // Both species and every packed growth progress: competition, water limits,
    // regrowth and immature stems must all span the complete configured palette.
    for (uint height=2u;height<=8u;++height) {
        for (uint level=0u;level<=15u;++level) {
            for (uint progress=0u;progress<=255u;++progress) {
                float factor=min(float(progress)/255.0,float(level)/15.0);
                float visible=grassGrownHeight(height,factor);
                if(visible==0.0)continue; // Geometry emits no color.
                int top=int(visible)-1;
                if(grassColorGradient(top,visible)!=1.0)return 1;
                if(visible>1.0&&grassColorGradient(0,visible)!=0.0)return 2;
                float previous=-1.0;
                for(int layer=0;layer<=top;++layer) {
                    float t=grassColorGradient(layer,visible);
                    float expected=visible==1.0?1.0:float(layer)/float(top);
                    if(abs(t-expected)>1e-6||t<previous)return 3;
                    float3 color=lerp(bottom,tip,t);
                    if(layer==top&&any(abs(color-tip)>1e-6))return 4;
                    if(layer==0&&visible>1.0&&any(abs(color-bottom)>1e-6))return 5;
                    previous=t;
                }
            }
        }
    }
    // A three-layer juvenile and a three-layer competition-limited plant have
    // exactly the same bottom/middle/tip colors, regardless of mature size.
    float juvenile=grassGrownHeight(8u,0.4);
    float limited=grassGrownHeight(5u,0.6);
    if(juvenile!=3.0||limited!=3.0)return 6;
    for(int layer=0;layer<3;++layer)
        if(grassColorGradient(layer,juvenile)!=grassColorGradient(layer,limited))return 7;
    if(grassColorGradient(0,1.0)!=1.0)return 8;
    return 0;
}
