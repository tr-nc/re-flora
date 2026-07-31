static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;

[[vk::binding(23, 0)]]
RWTexture2D<float4> ddgi_global_sky_irradiance;

uint2 gutterSource(uint2 destination)
{
    uint last = DDGI_IRRADIANCE_STORED_SIDE - 1u;
    uint interiorLast = DDGI_IRRADIANCE_INTERIOR_SIDE;

    bool corner = (destination.x == 0u || destination.x == last) &&
                  (destination.y == 0u || destination.y == last);
    if (corner)
    {
        return uint2(destination.x > 0u ? 1u : interiorLast,
                     destination.y > 0u ? 1u : interiorLast);
    }
    if (destination.y == 0u || destination.y == last)
    {
        return uint2(last - destination.x,
                     destination.y > 0u ? interiorLast : 1u);
    }
    return uint2(destination.x > 0u ? interiorLast : 1u,
                 last - destination.y);
}

[shader("compute")]
[numthreads(10, 10, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint2 destination = dispatchThreadId.xy;
    uint last = DDGI_IRRADIANCE_STORED_SIDE - 1u;
    bool isBorder = destination.x == 0u || destination.x == last ||
                    destination.y == 0u || destination.y == last;
    if (!isBorder) return;

    ddgi_global_sky_irradiance[destination] =
        ddgi_global_sky_irradiance[gutterSource(destination)];
}
