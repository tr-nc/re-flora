import vegetation_water_types;
import vegetation_water_solver;
import flora_types;
export __extern_cpp int main() {
    uint failures=0;
    for(uint species=0;species<10;++species) {
        WaterResponseState a=(WaterResponseState)0,b=a;
        // Same physical second at 30/120 Hz; no frame-rate-dependent impulses.
        for(uint n=0;n<30;++n)a=advanceWaterResponse(a,float4(0.2,-0.1,0.1,1.0),1.0/30.0,species,7);
        for(uint n=0;n<120;++n)b=advanceWaterResponse(b,float4(0.2,-0.1,0.1,1.0),1.0/120.0,species,7);
        if(abs(a.displacement_wetness.w-b.displacement_wetness.w)>1e-5 ||
            length(a.displacement_wetness.xyz-b.displacement_wetness.xyz)>0.04)failures++;
        WaterResponseState paused=advanceWaterResponse(a,float4(10),0,species,7);
        if(any(paused.displacement_wetness!=a.displacement_wetness)||any(paused.velocity!=a.velocity))failures++;
        bool reversed=false;
        float first=a.velocity.x;
        for(uint n=0;n<120;++n) {
            a=advanceWaterResponse(a,float4(0),1.0/120.0,species,7);
            reversed=reversed || first*a.velocity.x<0;
        }
        if(!reversed || a.displacement_wetness.w<=0.5)failures++;
        for(uint n=0;n<14400;++n)a=advanceWaterResponse(a,float4(0),1.0/120.0,species,7);
        if(length(a.displacement_wetness)>1e-4||length(a.velocity)>1e-4)failures++;
        WaterResponseState opposition=advanceWaterResponse((WaterResponseState)0,float4(0,0,0,2),1,species,7);
        if(opposition.displacement_wetness.w<0.9)failures++;
        WaterResponseState dry=advanceWaterResponse((WaterResponseState)0,float4(0),60,species,7);
        if(any(dry.displacement_wetness!=float4(0)))failures++;
        WaterResponseState intense=advanceWaterResponse((WaterResponseState)0,float4(10000),1,species,7);
        if(!all(isfinite(intense.displacement_wetness))||length(intense.displacement_wetness.xyz)>6)failures++;
    }
    VegetationWaterLobe lobe=(VegetationWaterLobe)0;
    lobe.center_exposure=float4(0,0,0,1);lobe.radius=float4(1);
    lobe.velocity=float4(1,0,0,0);lobe.lower=float4(-1);lobe.upper=float4(1);
    // Finite support and the producer's terrain face are authoritative.
    lobe.upper.x=0.1;
    if(waterLobeInput(lobe,float3(0.11,0,0)).w!=0 ||
       waterLobeInput(lobe,float3(0.0)).w<=0 ||
       waterLobeInput(lobe,float3(0,2,0)).w!=0)failures++;
    printf("vegetation water failures=%u\n",failures);return int(failures!=0);
}
