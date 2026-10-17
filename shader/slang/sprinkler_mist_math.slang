module sprinkler_mist_math;

// Per world unit; the saved density remains a dimensionless visual multiplier.
public static const float MIST_EXTINCTION = 2.2;

// A continuous density field, not sprites, droplets, or camera-facing cards.
public bool mistRayInterval(float3 origin, float3 direction, float3 center,
                           float3 radii, float sceneDistance, out float2 interval)
{
    interval = float2(0);
    float3 o = (origin - center) / radii;
    float3 d = direction / radii;
    float a = dot(d, d), b = dot(o, d), c = dot(o, o) - 1.0;
    float discriminant = b*b - a*c;
    if (discriminant <= 0.0 || a <= 0.0 || sceneDistance <= 0.0) return false;
    float root = sqrt(discriminant);
    interval = float2(max(0.0, (-b-root)/a), min(sceneDistance, (-b+root)/a));
    return interval.y > interval.x;
}

public float mistDensityAt(float3 position, float3 base, float3 center,
                          float3 radii, float time, float phase)
{
    float3 q = (position-center)/radii;
    float envelope = max(0.0, 1.0-dot(q,q));
    // The support and its derivative vanish at the edge. Broad, slow advection
    // only: no high-frequency screen noise or isolated bright grains.
    float waft = 0.64 + 0.24*sin(dot(q,float3(3.1,2.2,1.3))-time*0.7+phase*6.2831853)
                      + 0.12*sin(dot(q,float3(-1.4,4.0,2.5))-time*0.43);
    return envelope*envelope * waft * smoothstep(0.0,0.012,position.y-base.y);
}

public float mistPhase(float cosine)
{
    // A normalized mixture: directional water-mist scattering plus a broad
    // fine-aerosol lobe. Ordinary side/front views must still read moist air;
    // concentrating almost all energy into the sun-facing peak hid the effect.
    // cosine=1 means looking toward the light, not with it behind the camera.
    const float g = 0.65;
    float base = 1.0+g*g-2.0*g*clamp(cosine,-1.0,1.0);
    float forward = (1.0-g*g)/(12.5663706*base*sqrt(base));
    return 0.6*forward + 0.4/12.5663706;
}

public float3 mistComposite(float3 background, float opticalDepth, float3 integratedSource)
{
    if (opticalDepth <= 0.0) return background;
    // Avoid cancellation in 1-exp(-tau) at the soft support boundary.
    float weight = opticalDepth < 0.001
        ? 1.0-opticalDepth*0.5+opticalDepth*opticalDepth/6.0
        : (1.0-exp(-opticalDepth))/opticalDepth;
    return background*(1.0-opticalDepth*weight) + integratedSource*weight;
}
