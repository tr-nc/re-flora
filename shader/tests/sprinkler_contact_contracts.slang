import sprinkler_animation;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_spray_contact;

float flightToGround(MistSpraySource source,float3 target,out float3 point)
{
    float lo=0.0,hi=3.0;float3 velocity;
    for(uint i=0;i<26;++i) {
        float age=(lo+hi)*0.5;
        mistFlowAdvance(source.outlet,source.velocity,target,age,point,velocity);
        if(point.y>0.0)lo=age;else hi=age;
    }
    mistFlowAdvance(source.outlet,source.velocity,target,hi,point,velocity);
    point.y=0.0;return hi;
}

export __extern_cpp int main()
{
    uint failures=0;
    float speeds[3]={0,15,60};float angles[3]={10,40,90};
    float3 target=mistSprayTarget(float2(0),true);
    float worstMassError=0.0;
    for(uint speed=0;speed<3;++speed)for(uint angle=0;angle<3;++angle) {
        uint arrivals=0;float mass=0.0;
        for(uint n=0;n<48;++n) {
            MistSpraySource source=mistRotarySource(n,48,1.3,0.125,speeds[speed],angles[angle],1,0.18);
            float3 hit;float age=flightToGround(source,target,hit);
            if(!sprinklerArrivesInBin(age,source))continue;
            arrivals++;
            B_SprinklerWaterContact contact=sprinklerWaterContact(source,hit,float3(0,1,0),target,0.05);
            float density=sprinklerContactDensity(contact,hit-float3(0,0.5/256.0,0));
            // Original failure: the visible first contact received ZERO water.
            if(density<=0 || !isfinite(density))failures++;
            if(sprinklerContactDensity(contact,float3(0,-0.5/256.0,0))!=0)failures++;
            if(sprinklerContactDensity(contact,hit-float3(0,2.0/256.0,0))!=0)failures++;
            if(sprinklerContactDensity(contact,hit+float3(0,0.5/256.0,0))!=0)failures++;
            float3 lo,hi;sprinklerContactBounds(contact,lo,hi);
            float step=0.002;
            for(float z=lo.z+step*0.5;z<hi.z;z+=step)
                for(float x=lo.x+step*0.5;x<hi.x;x+=step)
                    mass+=sprinklerContactDensity(contact,float3(x,-0.5/256.0,z))*step*step;
            printf("rpm=%.0f angle=%.0f contact=(%.5f,%.5f) flight=%.5f wet_density=%.3f integral=%.5f\n",
                speeds[speed],angles[angle],hit.x,hit.z,age,density,mass);
        }
        worstMassError=max(worstMassError,abs(mass-1.0));
        if(arrivals!=1 || abs(mass-1.0)>0.003)failures++;
    }
    // The same birth direction persists as a pulse travels down the curved spray.
    MistSpraySource a=mistRotarySource(8,48,1.3,0.2,37,40,1,0.1);
    MistSpraySource b=mistRotarySource(21,48,1.3,0.2,37,40,1,0.1);
    b=mistRotarySource(21,48,1.3+(b.age-a.age)*a.angular_velocity,0.2,37,40,1,0.1);
    if(length(a.outlet-b.outlet)>1e-6 || length(a.velocity-b.velocity)>1e-5)failures++;

    // Duty is emission mass, NOT a geometric mask applied after depositing.
    // Average across whole cycles and several time steps, including < one bin.
    float duties[5]={0,0.1,0.5,0.9,1};
    for(uint d=0;d<5;++d)for(uint fps=30;fps<=120;fps*=2) {
        float mean=0;
        for(uint t=0;t<fps;++t) {
            MistSpraySource s=mistRotarySource(24,48,1.3,0.2,15,40,duties[d],float(t)/float(fps));
            mean+=s.emission*0.5/float(fps);
            if(duties[d]==0 && s.emission!=0)failures++;
            if(duties[d]==1 && s.emission!=2)failures++;
        }
        if(abs(mean-duties[d])>0.015)failures++;
        printf("duty=%.2f fps=%u integrated_input=%.5f\n",duties[d],fps,mean);
    }
    // Wind changes the shared flight/contact, not an independently shifted decal.
    MistSpraySource s=mistRotarySource(24,48,0,0,15,40,1,0);
    float3 calm,windy;float age=flightToGround(s,target,calm);
    float2 wind=float2(0.06,-0.04);
    flightToGround(s,mistSprayTarget(wind,true),windy);
    float travel=age-MIST_FLOW_DRAG_TIME*(1-exp(-age/MIST_FLOW_DRAG_TIME));
    if(length((windy-calm).xz-wind*travel)>1e-5)failures++;
    printf("shared spray/contact failures=%u worst_area_integral_error=%.6f\n",failures,worstMassError);
    return int(failures!=0);
}
