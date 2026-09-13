import leaf_flutter;

export __extern_cpp int main()
{
    float3 global = float3(2.0, 0.0, -1.0);
    float3 local = leafLocalOffset(11u, 0.4);
    if (any(leafWindOffset(global, local, 0.0) != local)) return 1;
    if (any(leafWindOffset(global, float3(0.0), 1.0) != global)) return 2;
    if (any(leafWindOffset(global, float3(0.0), 0.0) != float3(0.0))) return 3;
    if (length(leafWindOffset(global, local, 2.0) - (2.0 * global + local)) > 0.000001) return 4;
    if (leafTorqueTarget(float2(1.0, 0.2), 0.0, 11u, 0.0) != 0.0) return 5;
    if (any(leafLocalOffset(11u, 0.0) != float3(0.0))) return 6;
    return 0;
}
