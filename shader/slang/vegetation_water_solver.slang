module vegetation_water_solver;
import vegetation_water_types;
import flora_types;

// Exposure and momentum are separate: equal opposing sprays still wet a plant.
public float4 waterLobeInput(VegetationWaterLobe lobe,float3 position) {
    if(lobe.center_exposure.w<=0 || any(position<lobe.lower.xyz) || any(position>lobe.upper.xyz))
        return float4(0);
    float3 d=(position-lobe.center_exposure.xyz)/max(lobe.radius.xyz,float3(1e-6));
    float kernel=saturate(1.0-dot(d,d));
    float q=lobe.center_exposure.w*kernel*kernel*8.0;
    return float4(lobe.velocity.xyz*q,q);
}
public WaterResponseState advanceWaterResponse(WaterResponseState state,float4 input,float dt,uint species,uint seed) {
    if(dt<=0)return state;
    float rate=max(input.w,0.0)*2.5;
    const float drying=0.12;
    float equilibrium=rate/(rate+drying);
    float wet=equilibrium+(state.displacement_wetness.w-equilibrium)*exp(-(rate+drying)*dt);
    bool grass=species<2u, fruit=species==FLORA_SPECIES_APPLE;
    float height=species==FLORA_SPECIES_SHORT_GRASS?3.0:5.0;
    float angle=float(seed&65535u)*(6.2831853/65536.0);
    float3 rest=grass?float3(cos(angle)*0.65,-0.6,sin(angle)*0.65)*height*0.45:
        float3(0,fruit?-0.3:-0.95,0);
    // Saturated target prevents overlap counts from sending a stem unbounded.
    float3 impact=input.xyz*(grass?8.0:(fruit?2.0:5.0));
    impact/=1.0+length(impact)/(grass?3.0:1.8);
    float3 target=rest*wet+impact;
    float frequency=(grass?2.4:(fruit?1.7:2.8))/sqrt(1.0+(fruit?0.0:0.55)*wet);
    float damping=lerp(0.22,0.38,wet);
    float omega=6.2831853*frequency, decay=damping*omega;
    float oscillation=omega*sqrt(1.0-damping*damping);
    float3 q=state.displacement_wetness.xyz-target, v=state.velocity.xyz;
    float c=cos(oscillation*dt), s=sin(oscillation*dt)/oscillation, envelope=exp(-decay*dt);
    state.displacement_wetness=float4(target+envelope*(q*c+(v+decay*q)*s),saturate(wet));
    state.velocity=float4(envelope*(v*c-(decay*v+omega*omega*q)*s),0);
    return state;
}
