import sprinkler_animation;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_spray_contact;

static const uint COUNT=256u;
static const uint AGES=COUNT/MIST_ROTARY_BANDS;

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

float integrateSurface(B_SprinklerWaterContact contact,inout uint failures)
{
    float3 lo,hi;sprinklerContactBounds(contact,lo,hi);
    uint axis=abs(contact.normal.x)>0.5?0u:(abs(contact.normal.y)>0.5?1u:2u);
    float2 lower=axis==0?lo.yz:(axis==1?lo.xz:lo.xy);
    float2 upper=axis==0?hi.yz:(axis==1?hi.xz:hi.xy);
    float2 padding=(upper-lower)*0.1;
    lower-=padding;upper+=padding;
    float2 step=(upper-lower)/128.0;
    float mass=0;
    for(uint v=0;v<128;++v)for(uint u=0;u<128;++u) {
        float2 uv=lower+(float2(u,v)+0.5)*step;
        float3 p=contact.center_flux.xyz-contact.normal.xyz*(0.5/256.0);
        if(axis==0)p.yz=uv;else if(axis==1)p.xz=uv;else p.xy=uv;
        float density=sprinklerContactDensity(contact,p);
        if(!isfinite(density) || (density>1e-5 && (any(p<lo) || any(p>hi))))failures++;
        mass+=density*step.x*step.y;
    }
    return mass;
}

export __extern_cpp int main()
{
    uint failures=0;
    float speeds[3]={0,15,60};float angles[3]={10,40,90};
    float3 target=mistSprayTarget(float2(0),true);
    float worstMassError=0.0;
    uint arrivalNodes[MIST_ROTARY_BANDS];
    for(uint speed=0;speed<3;++speed)for(uint angle=0;angle<3;++angle) {
        uint arrivals=0;float mass=0.0;
        for(uint n=0;n<COUNT;++n) {
            MistSpraySource source=mistRotarySource(n,COUNT,1.3,0.125,speeds[speed],angles[angle],1,0.18);
            float3 hit;float age=flightToGround(source,target,hit);
            if(!sprinklerArrivesInBin(age,source))continue;
            if(speed==0 && angle==0)arrivalNodes[arrivals]=n;
            arrivals++;
            B_SprinklerWaterContact contact=sprinklerWaterContact(source,hit,float3(0,1,0),target,0.05,age);
            float density=sprinklerContactDensity(contact,hit-float3(0,0.5/256.0,0));
            // Original failure: the visible first contact received ZERO water.
            if(density<=0 || !isfinite(density))failures++;
            if(sprinklerContactDensity(contact,hit+float3(1,-0.5/256.0,1))!=0)failures++;
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
        if(arrivals!=MIST_ROTARY_BANDS || abs(mass-1.0)>0.003)failures++;
    }
    // The same birth direction persists as a pulse travels down the curved spray.
    MistSpraySource a=mistRotarySource(8,COUNT,1.3,0.2,37,40,1,0.1);
    MistSpraySource b=mistRotarySource(21,COUNT,1.3,0.2,37,40,1,0.1);
    b=mistRotarySource(21,COUNT,1.3+(b.age-a.age)*a.angular_velocity,0.2,37,40,1,0.1);
    if(length(a.outlet-b.outlet)>1e-6 || length(a.velocity-b.velocity)>1e-5)failures++;

    // Duty is emission mass, NOT a geometric mask applied after depositing.
    // Average across whole cycles and several time steps, including < one bin.
    float duties[5]={0,0.1,0.5,0.9,1};
    for(uint d=0;d<5;++d)for(uint fps=30;fps<=120;fps*=2) {
        float mean=0;
        for(uint t=0;t<fps;++t) {
            float emission=0;
            for(uint band=0;band<MIST_ROTARY_BANDS;++band) {
                MistSpraySource s=mistRotarySource(arrivalNodes[band],COUNT,1.3,0.2,15,40,duties[d],float(t)/float(fps));
                emission+=s.emission;
            }
            mean+=emission*0.5/float(fps);
            if(duties[d]==0 && emission!=0)failures++;
            if(duties[d]==1 && abs(emission-2)>1e-5)failures++;
        }
        if(abs(mean-duties[d])>0.015)failures++;
        printf("duty=%.2f fps=%u integrated_input=%.5f\n",duties[d],fps,mean);
    }
    // Wind changes the shared flight/contact, not an independently shifted decal.
    MistSpraySource s=mistRotarySource(AGES/2,COUNT,0,0,15,40,1,0);
    float3 calm,windy;float age=flightToGround(s,target,calm);
    float2 wind=float2(0.06,-0.04);
    flightToGround(s,mistSprayTarget(wind,true),windy);
    float travel=age-MIST_FLOW_DRAG_TIME*(1-exp(-age/MIST_FLOW_DRAG_TIME));
    if(length((windy-calm).xz-wind*travel)>1e-5)failures++;
    // Oblique wall/ceiling hits and the numerical grazing guard conserve the
    // arriving mass too. Padded integration also detects incorrect footprint
    // bounds or a half-voxel sideways drift into the soil layer.
    MistSpraySource oblique=mistRotarySource(COUNT-AGES+8,COUNT,0,0,0,40,1,0);
    float3 normals[4]={float3(-1,0,0),float3(0,0,1),float3(0,-1,0),float3(0,1,0)};
    float apex=MIST_FLOW_DRAG_TIME*log((oblique.velocity.y-target.y)/(-target.y));
    float times[4]={0.15,0.15,0.04,apex};
    for(uint face=0;face<4;++face) {
        float3 hit,velocity;mistFlowAdvance(oblique.outlet,oblique.velocity,target,times[face],hit,velocity);
        B_SprinklerWaterContact contact=sprinklerWaterContact(oblique,hit,normals[face],target,0.05,times[face]);
        float error=abs(integrateSurface(contact,failures)/contact.center_flux.w-1.0);
        worstMassError=max(worstMassError,error);
        if(error>0.003)failures++;
    }
    printf("shared spray/contact failures=%u worst_area_integral_error=%.6f\n",failures,worstMassError);
    return int(failures!=0);
}
