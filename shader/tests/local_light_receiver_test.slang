import local_light_field_query;
import local_light_field_policy;
import local_lighting;
import local_emitter;

bool near(float3 a, float3 b) { return all(abs(a-b) <= float3(1e-5) + abs(b)*1e-5); }
export __extern_cpp int main()
{
    uint failures = 0u, cases = 0u;
    U_LightGpu light = {};
    light.abi_version=LOCAL_LIGHT_GPU_ABI_VERSION;
    light.kind=LOCAL_LIGHT_KIND_POINT;
    light.position=float3(.0625,.05,.0625);
    light.range=1.0;light.source_radius=1.0/256.0;
    light.intensity=1.0;light.color=float3(1.0,.5,.2);
    // The original all-clear 32-voxel cage returns zero for the reported
    // below-source receiver. Analytic receiver terms cannot lose this peak.
    for(uint i=0u;i<256u;++i)
    {
        float3 p=float3(float(i%16u)/128.0, .0125, float(i/16u)/128.0);
        uint tap;
        float3 actual=localLightReceiverSample(light,p,float3(0,1,0),0u,tap);
        float3 expected=evaluateUnshadowedPointLight(light,p,float3(0,1,0)).irradiance;
        if(!near(actual,expected)||!any(actual>float3(0.0))||tap!=0u) failures++;
        cases++;
    }
    // Range/cosine changes are evaluated at the receiver, never inherited from
    // a probe on the other side of the source, horizon, or range boundary.
    for(uint i=0u;i<64u;++i)
    {
        float3 p=float3(.0625,.05-float(i+1u)/32.0,.0625);
        uint tap;
        if(!near(localLightReceiverSample(light,p,float3(0,1,0),0u,tap),
            evaluateUnshadowedPointLight(light,p,float3(0,1,0)).irradiance))failures++;
        if(any(localLightReceiverSample(light,p,float3(0,-1,0),0u,tap)!=float3(0.0)))failures++;
        cases++;
    }
    // Fixed face identities preserve all original finite quadrature samples for
    // every octant, including receivers crossing the emitting box center planes.
    light.shape_params=float4(.01,.008,.006,1.0);
    for(uint octant=0u;octant<8u;++octant)
    {
        float3 signs=float3((octant&1u)?1:-1,(octant&2u)?1:-1,(octant&4u)?1:-1);
        float3 p=light.position+signs*float3(.15,.12,.19);
        for(uint tap=0u;tap<12u;++tap)
        {
            uint fixedTap;
            float3 actual=localLightReceiverSample(light,p,normalize(-signs),tap,fixedTap);
            LocalEmitterSample fixed=sampleLocalEmitterFixedIncident(light.position,
                light.shape_params.xyz,light.range,p,fixedTap);
            LocalEmitterSample original=sampleLocalEmitter(light.position,
                light.shape_params.xyz,light.range,p,normalize(-signs),tap);
            float cosine=max(0.0,dot(normalize(-signs),normalize(fixed.position-p)));
            if(!near(actual,light.color*light.intensity*fixed.irradiance_per_intensity*cosine)||
                !near(original.position,fixed.position)||fixedTap>=24u)failures++;
            float entry=localEmitterConnectionDistance(p,light.position,light.shape_params.xyz,fixed.position);
            if(abs(entry-length(fixed.position-p))>1e-5)failures++;
            cases++;
        }
    }
    // RGB transport packing preserves colors, opaque zero and clear one. A
    // missing record is distinct from a real blocked connection.
    for(uint i=0u;i<=100u;++i)
    {
        float3 t=float3(float(i)/100.0,.2,.8);
        float4 decoded=unpackLocalLightTransmission(packLocalLightTransmission(t));
        if(any(abs(t-decoded.xyz)>float3(1.0/65535.0))||decoded.w!=1.0)failures++;
        cases++;
    }
    if(unpackLocalLightTransmission(uint2(0)).w!=0.0)failures++;
    for(uint kind=0u;kind<5u;++kind)
    {
        LocalLightTransmissionSupport s=emptyLocalLightTransmissionSupport();
        for(uint i=0u;i<8u;++i)
        {
            float3 t=kind==0u?float3(1.0):kind==1u?float3(.2,.5,.8):float3(0.0);
            if(kind==3u&&i==0u)t=float3(1.0);
            float4 record=unpackLocalLightTransmission(packLocalLightTransmission(t));
            if(kind==4u&&i==0u)record.w=0.0;
            accumulateLocalLightTransmission(s,1.0/8.0,record);
        }
        float3 t;
        bool accepted=finishLocalLightTransmission(s,1.0,t);
        if(accepted!=(kind<2u))failures++;
        if(kind==1u&&!near(t,float3(.2,.5,.8)))failures++;
    }
    // Near-source topology and incomplete/blocked support require real scene
    // visibility, never a synthetic brightness floor or an interpolated shadow.
    if(localLightOutsideReceiverSupport(float3(0),float3(.05,0,0),.01,.125*.125))failures++;
    if(!localLightOutsideReceiverSupport(float3(0),float3(.5,0,0),.01,.125*.125))failures++;
    if(localLightConnectionIndex(2u,3u,8u,23u)!=2u+(2u*8u+3u)*24u+23u)failures++;
    printf("local_light_receiver spacing=32 cases=%u failures=%u\n",cases,failures);
    return failures==0u?0:1;
}
