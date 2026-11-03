import light_reservoir;
import local_emitter;

// Enumerate every two-proposal sequence and reservoir selection interval. This
// checks the estimator's expectation, not just its internal sum of weights.
export __extern_cpp int main()
{
    float3 response[2] = { float3(4,1,0), float3(0,2,8) };
    float3 transmission[2] = { float3(0), float3(0.25,0.5,0.75) };
    float probabilities[2] = { 0.25, 0.75 };
    float3 expected = response[0]*transmission[0]+response[1]*transmission[1];
    float3 measured = float3(0);
    for(uint a=0; a<2; ++a) for(uint b=0; b<2; ++b)
    {
        for(uint r=0; r<4096; ++r)
        {
            LightReservoir reservoir = {};
            streamLightCandidate(reservoir,a,lightSamplingTarget(response[a]),
                1.0/probabilities[a],0.5);
            streamLightCandidate(reservoir,b,lightSamplingTarget(response[b]),
                1.0/probabilities[b],(float(r)+0.5)/4096.0);
            uint selected=reservoir.selected;
            measured += response[selected]*transmission[selected] *
                lightReservoirNormalization(reservoir,2u) *
                (probabilities[a]*probabilities[b]/4096.0);
        }
    }
    bool energy=all(abs(measured-expected)<float3(0.002));
    LightReservoir empty = {};
    streamLightCandidate(empty,0u,0.0,1.0,0.0);
    bool zero=lightReservoirNormalization(empty,8u)==0.0;
    LightReservoir sparse = {};
    streamLightCandidate(sparse,0u,2.0,4.0,0.5);
    bool zeroCount=lightReservoirNormalization(sparse,8u)==0.5;
    bool finite=true;
    float3 center=float3(0.2,0.3,2), half=float3(0.4,0.4,0.1);
    // Enumerate uniform quadrature proposals: multiplying by twelve must
    // recover the SAME finite emitter, including zero and backfacing taps.
    for(uint caseIndex=0;caseIndex<4u;++caseIndex)
    {
        float3 normal=caseIndex==1u ? float3(0,0,-1) : float3(0,0,1);
        float range=caseIndex==2u ? 0.5 : 100.0;
        float reference=0.0, estimate=0.0;
        for(uint tap=0;tap<12u;++tap)
        {
            LocalEmitterSample s=sampleLocalEmitter(center,half,range,float3(0),normal,tap);
            float v=caseIndex==3u ? (s.position.x>0.0 ? 0.25 : 0.0) : 1.0;
            reference+=s.irradiance_per_intensity*v;
            LightReservoir reservoir={};
            streamLightCandidate(reservoir,tap,s.irradiance_per_intensity,12.0,0.5);
            estimate+=s.irradiance_per_intensity*v*
                lightReservoirNormalization(reservoir,1u)/12.0;
        }
        finite=finite && abs(estimate-reference)<1.0e-6;
    }
    printf("RIS energy=%d measured=(%f,%f,%f) zero=%d zero_proposals_count=%d finite_emitter=%d\n",
        energy,measured.x,measured.y,measured.z,zero,zeroCount,finite);
    return energy && zero && zeroCount && finite ? 0 : 1;
}
