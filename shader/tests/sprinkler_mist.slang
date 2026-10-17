import sprinkler_mist_math;

bool near(float a,float b) { return abs(a-b)<0.00001; }
float3 compositeMedium(float3 background,float tau,float3 source) {
    return mistComposite(background,tau,source*tau);
}

export __extern_cpp int main()
{
    float3 origin=float3(0,0,-2),direction=float3(0,0,1),center=float3(0);
    float2 interval;
    // Opaque geometry before / inside / after the volume clips the integrated
    // path, including the camera-inside and camera-looking-away cases.
    if(mistRayInterval(origin,direction,center,float3(1),0.5,interval))return 1;
    if(!mistRayInterval(origin,direction,center,float3(1),2,interval) ||
       !near(interval.x,1)||!near(interval.y,2))return 2;
    if(!mistRayInterval(origin,direction,center,float3(1),10,interval)||
       !near(interval.x,1)||!near(interval.y,3))return 3;
    if(!mistRayInterval(center,direction,center,float3(1),10,interval)||
       !near(interval.x,0)||!near(interval.y,1))return 4;
    if(mistRayInterval(origin,-direction,center,float3(1),10,interval))return 5;
    if(mistRayInterval(float3(2,0,-2),direction,center,float3(1),10,interval))return 6;

    float3 base=float3(0),c=float3(0,0.058,0),r=float3(0.28,0.105,0.28);
    for(uint i=0;i<120;++i) {
        float time=float(i)*0.05;
        float density=mistDensityAt(c,base,c,r,time,0.35);
        if(!isfinite(density)||density<0.0||density>1.0)return 7;
        if(mistDensityAt(float3(0,-0.001,0),base,c,r,time,0.35)!=0.0)return 8;
        if(mistDensityAt(c+r*float3(1.01,0,0),base,c,r,time,0.35)!=0.0)return 9;
        if(mistDensityAt(c+r*float3(0.999,0,0),base,c,r,time,0.35)>0.00001)return 10;
        float next=mistDensityAt(c,base,c,r,time+0.001,0.35);
        if(abs(next-density)>0.001)return 11; // no temporal speckle / popping
        if(abs(mistDensityAt(c+float3(0.00001,0,0),base,c,r,time,0.35)-density)>0.001)return 12;
    }
    if(mistPhase(1)<mistPhase(0)*10 || mistPhase(0)<=mistPhase(-1))return 13;
    // HG integrates to one over the sphere: it redistributes light, not emission.
    float integral=0.0;
    for(uint s=0;s<8192;++s)integral+=mistPhase(-1+(float(s)+0.5)*2/8192)*12.5663706/8192;
    if(abs(integral-1.0)>0.001)return 14;
    float3 background=float3(0.3,0.6,0.9);
    if(any(mistComposite(background,0,float3(0))!=background))return 15;
    if(any(mistComposite(float3(0),1,float3(0))!=float3(0)))return 16; // dark air never glows
    for(float tau=0.00000001;tau<30;tau*=2) {
        float3 result=compositeMedium(background,tau,float3(0.5));
        if(any(!isfinite(result))||any(result<0.0)||any(result>float3(0.90001)))return 17;
        float3 dark=compositeMedium(background,tau,float3(0));
        if(length(dark-background*exp(-tau))>0.00001)return 18;
    }
    float3 once=compositeMedium(background,0.3,float3(0.5));
    float3 twice=compositeMedium(compositeMedium(background,0.1,float3(0.5)),0.2,float3(0.5));
    if(length(once-twice)>0.00001)return 19;
    // Water is forward-scattering: stronger cue toward the sun, subtle with the
    // sun behind the viewer. Removing all light cannot leave a white fog card.
    float3 lit=compositeMedium(float3(0.1),0.1,float3(mistPhase(1)));
    float3 front=compositeMedium(float3(0.1),0.1,float3(mistPhase(-1)));
    if(any(lit<=front))return 20;
    return 0;
}
