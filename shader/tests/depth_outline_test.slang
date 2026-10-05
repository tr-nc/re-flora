import depth_outline_filter;
import scene_depth;
bool close(float a,float b){return abs(a-b)<0.001;}
OutlineSample sample(float reciprocal,float screenX)
{
    OutlineSample s={reciprocal,float3(0)};
    if(reciprocal>0)s.viewPosition=float3(screenX/reciprocal,0,-1/reciprocal);
    return s;
}
float pair(float c,float a,float b,float a2,float b2)
{
    return outlinePair(sample(c,0),sample(a,-0.1),sample(b,0.1),a2,b2,float4(0.04,0.004,0.5,0.70710678),0.6);
}
export __extern_cpp int main()
{
    if(pair(1,1,1,1,1)!=0)return 1;
    if(pair(1,1.5,0.5,2,0)!=0)return 2; // steep projected plane
    if(pair(0.1,0.15,0.05,0.2,0)!=0)return 3; // distant plane
    if(!close(pair(1,1,0.5,1,0.5),1))return 4; // near side of jump
    if(pair(0.5,1,0.5,1,0.5)!=0)return 5; // no background halo
    if(pair(0,1,0,1,0)!=0)return 6;
    if(!close(pair(1,1,0,1,0),0.6))return 7;
    if(pair(1,1,0.999,1,0.999)!=0)return 8;
    // Both polarities of a continuous 90 degree terrain/model fold.
    if(pair(1,1.1,1.1,1.2,1.2)<=0)return 9;
    if(pair(1,0.9,0.9,0.8,0.8)<=0)return 10;
    for(uint i=0;i<10;i++) {
        float a=0.5+float(i)*0.1,b=1.4-float(i)*0.1;
        if(!close(pair(1,a,b,2*a-1,2*b-1),pair(1,b,a,2*b-1,2*a-1)))return 11;
    }
    if(!close(outlineCoverage(1,4,0.25),0.25)||!close(outlineCoverage(1,1,0.25),1))return 12;
    float n=0.01,f=10;
    float4x4 inv=float4x4(1,0,0,0,0,1,0,0,0,0,0,-1,0,0,(n-f)/(f*n),1/n);
    float d1=f/(f-n)-f*n/(f-n),d2=f/(f-n)-f*n/(2*(f-n));
    if(!close(outlineSample(d1,float2(0.5),inv).inverseDepth,1))return 13;
    if(!close(outlineSample(d2,float2(0.01,0.98),inv).inverseDepth,0.5))return 14;
    if(outlineSample(1,float2(0.5),inv).inverseDepth!=0)return 15;
    if(!close(sceneVisibleDepth(0.2,1,0.8),0.2))return 16;
    if(!close(sceneVisibleDepth(0.8,1,0.2),0.2))return 17;
    if(!close(sceneVisibleDepth(0.2,0.3,0.8),0.2))return 18;
    if(!close(sceneVisibleDepth(1,0,0.8),0.8))return 19; // tracer-only terrain
    if(sceneVisibleDepth(0.2,0,1)!=1)return 20;
    if(!close(sceneVisibleDepth(-1,1,0.8),0.8))return 21;
    // Outer samples crossing a nearby fold must not thicken a planar center.
    if(pair(1,1,1,1.2,1)!=0)return 22;
    // A large artistic threshold must not create a crease on the farther
    // side of a modest overlapping leaf/object discontinuity.
    if(outlinePair(sample(0.8,0),sample(1,-0.1),sample(0.8,0.1),1,0.8,
        float4(0.5,0.004,0.5,0.70710678),0.6)!=0)return 23;
    return 0;
}
