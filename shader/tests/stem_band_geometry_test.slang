import stem_band_geometry;
bool close(float3 a,float3 b) {return all(abs(a-b)<2e-5);}
export __extern_cpp int main() {
    float3 a=float3(0),b=float3(0,2,0),up=float3(0,1,0),side=float3(1,0,0);
    for(uint corner=0u;corner<8u;corner++) {
        float3 q=stemBandCorner(corner);
        float3 p=stemBandPoint(a,b,up,up,side,float2(0.5),float2(0.5),q);
        if(!close(p,float3(q.x,2*(q.y+0.5),q.z)))return 1;
    }
    // Shared boundary frame/radius yields identical rings across bent bands.
    float3 bend=normalize(float3(0.7,1,0.3)),joint=float3(0.3,2,0.1);
    for(uint corner=0u;corner<8u;corner++) {
        float3 q=stemBandCorner(corner);q.y=0.5;
        float3 first=stemBandPoint(a,joint,up,bend,side,float2(0.5),float2(0.3),q);
        q.y=-0.5;
        float3 second=stemBandPoint(joint,float3(1,3,0.6),bend,bend,side,float2(0.3),float2(0.2),q);
        if(!close(first,second))return 2;
    }
    for(uint i=0u;i<32u;i++) {
        float3 axis=normalize(float3(float(int(i)-16),1,float(i%5u)));
        float3 p=stemBandPoint(a,axis,axis,axis,axis,float2(0.5),float2(0.1),float3(0.5));
        if(any(isnan(p))||any(isinf(p)))return 3;
    }
    if(stemColorBand(0,10,8)!=0u||stemColorBand(10,10,8)!=7u)return 4;
    if(stemColorBand(2,10,8)!=1u||stemColorBand(2.5,10,8)!=2u)return 5;
    if(abs(stemBandTaper(2u,1)-0.12)>1e-5||stemBandTaper(1u,1)!=1)return 6;
    return 0;
}
