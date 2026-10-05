import depth_outline_filter;
bool close(float a,float b){return abs(a-b)<0.001;}
export __extern_cpp int main()
{
    // Flat and arbitrarily steep projected planes: inverse-depth gradients
    // cancel at both near and far distances, including diagonal pairs.
    if(outlinePair(1,1,1,0.04,0.004,0.5,0.6)!=0)return 1;
    if(outlinePair(1,0.1,1.9,0.04,0.004,0.5,0.6)!=0)return 2;
    if(outlinePair(0.1,0.08,0.12,0.04,0.004,0.5,0.6)!=0)return 3;
    if(!close(outlinePair(1,1,0.5,0.04,0.004,0.5,0.6),1))return 4;
    if(outlinePair(0.5,1,0.5,0.04,0.004,0.5,0.6)!=0)return 5;
    if(outlinePair(0,1,0,0.04,0.004,0.5,0.6)!=0)return 6;
    if(!close(outlinePair(1,1,0,0.04,0.004,0.5,0.6),0.6))return 7;
    if(outlinePair(1,1,0.99,0.04,0.004,0.5,0.6)!=0)return 8;
    if(outlinePair(100,100,99,0.001,0.004,0.5,0.6)!=0)return 9;
    if(!close(outlineCoverage(1,4,0.25),0.25)||!close(outlineCoverage(1,1,0.25),1))return 10;
    if(!outlineThinPair(1,0,0,0.04)||outlineThinPair(1,1,0,0.04))return 11;
    float n=0.01,f=10;
    float4x4 inverseProjection=float4x4(1,0,0,0,0,1,0,0,0,0,0,-1,0,0,(n-f)/(f*n),1/n);
    float d1=f/(f-n)-f*n/(f-n);
    float d2=f/(f-n)-f*n/(2*(f-n));
    if(!close(outlineInverseEyeDepth(d1,float2(0.5),inverseProjection),1))return 12;
    if(!close(outlineInverseEyeDepth(d2,float2(0.5),inverseProjection),0.5))return 13;
    if(outlineInverseEyeDepth(1,float2(0.5),inverseProjection)!=0)return 14;
    if(outlineInverseEyeDepth(-1,float2(0.5),inverseProjection)!=0)return 15;
    // Offset UV must not change eye depth for an ordinary perspective camera.
    if(!close(outlineInverseEyeDepth(d2,float2(0.01,0.98),inverseProjection),0.5))return 16;
    if(!close(outlineVisibleDepth(0.2,1,0.8),0.2))return 17;
    if(!close(outlineVisibleDepth(0.8,1,0.2),0.2))return 18;
    if(!close(outlineVisibleDepth(0.2,0.3,0.8),0.8))return 19;
    if(outlineVisibleDepth(0.2,0,1)!=1)return 20;
    if(!close(outlineVisibleDepth(-1,1,0.8),0.8))return 21;
    return 0;
}
