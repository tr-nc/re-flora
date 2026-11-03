module local_visibility_policy;

// An angular cell is a closed cube-face cone, not a point-sampled shadow map.
// Clear depth is a conservative lower bound on ANY published terrain surface
// in that cone. A second certificate names one actual opaque voxel. Ambiguous
// connections are evaluated by the original segment transport, per emitter tap.
public static const uint LOCAL_VISIBILITY_SIDE = 32u;
public static const uint LOCAL_VISIBILITY_CELLS = 6u * 32u * 32u;
public static const uint LOCAL_VISIBILITY_SOURCES = 24u;
public static const uint LOCAL_VISIBILITY_ABI = 1u;
public static const float LOCAL_VISIBILITY_EPSILON = 2.0e-5;

public bool visibilityHeaderMatches(uint4 header, uint4 geometry, uint records,
    uint lights, uint2 liveRevision, uint geometryRevision)
{
    uint perLight=LOCAL_VISIBILITY_CELLS*LOCAL_VISIBILITY_SOURCES;
    if(lights>(0xffffffffu-2u)/perLight) return false;
    return header.x==LOCAL_VISIBILITY_ABI && header.y==lights &&
        all(header.zw==liveRevision) && geometry.x==geometryRevision &&
        geometry.y==LOCAL_VISIBILITY_SIDE && geometry.z==1u &&
        records>=2u+lights*perLight;
}

public float3 visibilityFaceVector(uint face, float2 slope)
{
    uint axis = face / 2u;
    float3 d = float3(0.0);
    d[axis] = (face & 1u) != 0u ? 1.0 : -1.0;
    d[(axis + 1u) % 3u] = slope.x;
    d[(axis + 2u) % 3u] = slope.y;
    return d;
}

public uint visibilityCell(float3 segment, out uint face, out float forward)
{
    float3 a = abs(segment);
    uint axis = a.x >= max(a.y, a.z) ? 0u : (a.y >= a.z ? 1u : 2u);
    forward = a[axis];
    face = axis * 2u + (segment[axis] >= 0.0 ? 1u : 0u);
    float2 slope = float2(segment[(axis+1u)%3u], segment[(axis+2u)%3u]) / max(forward, 1.0e-20);
    uint2 p = uint2(clamp(floor((slope + 1.0) * (0.5 * LOCAL_VISIBILITY_SIDE)),
        float2(0.0), float2(LOCAL_VISIBILITY_SIDE - 1u)));
    return (face * LOCAL_VISIBILITY_SIDE + p.y) * LOCAL_VISIBILITY_SIDE + p.x;
}

public void visibilityCellSlopes(uint cell, out uint face, out float2 lo, out float2 hi)
{
    face = cell / (LOCAL_VISIBILITY_SIDE * LOCAL_VISIBILITY_SIDE);
    uint2 p = uint2(cell % LOCAL_VISIBILITY_SIDE, (cell / LOCAL_VISIBILITY_SIDE) % LOCAL_VISIBILITY_SIDE);
    lo = float2(p) * (2.0 / LOCAL_VISIBILITY_SIDE) - 1.0;
    hi = lo + 2.0 / LOCAL_VISIBILITY_SIDE;
}

// Plane rejection is conservative: passing does not assert intersection, but
// rejection guarantees every ray in the cell misses. Expanded bounds protect
// float-rounding at cube seams, voxel edges and source-facing planes.
public bool visibilityConeMayIntersect(float3 origin, uint face, float2 lo, float2 hi,
    float3 lower, float3 upper, float limit, out float nearDepth)
{
    lower -= origin + LOCAL_VISIBILITY_EPSILON;
    upper -= origin - LOCAL_VISIBILITY_EPSILON;
    uint a = face / 2u, u = (a+1u)%3u, v = (a+2u)%3u;
    float f0 = (face & 1u) != 0u ? lower[a] : -upper[a];
    float f1 = (face & 1u) != 0u ? upper[a] : -lower[a];
    nearDepth = max(0.0, f0);
    if (f1 < 0.0 || nearDepth >= limit) return false;
    return upper[u] >= min(lo.x*f0, lo.x*f1) &&
           lower[u] <= max(hi.x*f0, hi.x*f1) &&
           upper[v] >= min(lo.y*f0, lo.y*f1) &&
           lower[v] <= max(hi.y*f0, hi.y*f1);
}

public bool visibilitySegmentBox(float3 origin, float3 endpoint,
    float3 lower, float3 upper, out float entry, out float exit)
{
    entry = 0.0; exit = 1.0;
    float3 d = endpoint - origin;
    for (uint a=0u; a<3u; ++a)
    {
        if (abs(d[a]) < 1.0e-20)
        {
            if (origin[a] <= lower[a] || origin[a] >= upper[a]) return false;
        }
        else
        {
            float x=(lower[a]-origin[a])/d[a], y=(upper[a]-origin[a])/d[a];
            entry=max(entry,min(x,y)); exit=min(exit,max(x,y));
        }
    }
    return entry < exit;
}

// 1 = certified clear, 0 = certified opaque, -1 = exact transport required.
public int visibilityCertificate(float4 record, float3 source, float3 receiver, uint face)
{
    float3 d=receiver-source;
    float forward=abs(d[face/2u]);
    if (!all(isfinite(record)) || record.x < 0.0) return -1;
    if (forward + LOCAL_VISIBILITY_EPSILON < record.x) return 1;
    if (record.y >= 0.0)
    {
        float entry, exit;
        float3 halfSize=float3(0.5/256.0);
        if (visibilitySegmentBox(receiver, source, record.yzw-halfSize, record.yzw+halfSize, entry, exit))
        {
            float distance=length(d);
            // Exclude endpoint/boundary grazes the original marcher may skip.
            if (entry*distance > 2.0e-4 && exit*distance < distance-2.0e-4 &&
                (exit-entry)*distance > 2.0e-4) return 0;
        }
    }
    return -1;
}
