import sprinkler_air_math;
import sprinkler_mist_transport;
import sprinkler_mist_math;

export __extern_cpp int main()
{
    B_SprinklerAirInfo empty=airGridInfo(float3(1e20),float3(-1e20),96,float4(0));
    if(empty.lower_valid.w!=0.0 || any(!isfinite(empty.cell_side)))return 1;
    float3 lo=float3(-0.3,0.1,0.2),hi=float3(0.7,0.6,0.4);
    B_SprinklerAirInfo info=airGridInfo(lo,hi,96,float4(2,4,6,2));
    float3 end=info.lower_valid.xyz+info.cell_side.xyz*info.cell_side.w;
    if(any(info.lower_valid.xyz>lo) || any(end<hi))return 2;
    if(any(abs(info.ambient_mass.xyz-float3(1,2,3))>1e-6))return 3;
    if(airCellIndex(uint3(95),96)!=96u*96u*96u-1u)return 4;
    float2 interval;
    if(!airRayInterval(float3(-2,0.3,0.3),float3(1,0,0),10,info,interval))return 5;
    if(airRayInterval(float3(-2,9,0.3),float3(1,0,0),10,info,interval))return 6;
    if(airRayInterval(float3(-2,0.3,0.3),float3(1,0,0),0.1,info,interval))return 7;
    float3 center=(lo+hi)*0.5;
    if(!airRayInterval(center,float3(0,1,0),10,info,interval) || interval.x!=0)return 8;
    // Camera inside, parallel axes and an empty field never produce NaN/inf.
    if(airRayInterval(center,float3(1,0,0),10,empty,interval))return 9;
    B_SprinklerMistLobe lobe=(B_SprinklerMistLobe)0;
    lobe.center_amplitude.w=32;
    lobe.radii_near_geometry=float4(0.001,0.02,0.04,1);
    for(uint s=1;s<32;++s) {
        B_SprinklerMistLobe broad=airResampleLobe(lobe,float(s)*0.001);
        float3 r=lobe.radii_near_geometry.xyz,b=broad.radii_near_geometry.xyz;
        float mass=lobe.center_amplitude.w*r.x*r.y*r.z;
        float broadMass=broad.center_amplitude.w*b.x*b.y*b.z;
        if(abs(mass-broadMass)>mass*1e-5 || any(b<r))return 10;
    }
    if(airEncodeDensity(0)!=0 || airEncodeDensity(-1)!=0 ||
       airEncodeDensity(1)!=64 || airEncodeDensity(1e10)!=65535u)return 11;
    for(uint i=0;i<10000;++i) {
        float density=float(i)*0.01;
        float decoded=float(airEncodeDensity(density))/SPRINKLER_AIR_FIXED_SCALE;
        if(abs(density-decoded)>0.00782)return 12;
    }
    // Final integration remains Beer compositing, not a second alpha layer.
    float3 background=float3(0.1,0.2,0.3),mistSource=float3(0.5),dropSource=float3(1);
    float3 actual=mistComposite(background,0.7+0.2,mistSource*0.7+dropSource*0.2);
    if(any(!isfinite(actual)) || any(actual<background) || any(actual>float3(1)))return 13;
    return 0;
}
