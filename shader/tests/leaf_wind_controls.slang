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
    if (length(leafLocalDisplacement(11u, 0.4, 0.36627253) - local) > 0.000001) return 7;
    if (length(leafLocalDisplacement(11u, 0.4, 0.0)) != 0.0) return 8;
    if (length(leafLocalDisplacement(11u, 0.0, 5.0)) != 0.0) return 9;
    if (abs(length(leafLocalDisplacement(11u, 0.75, 5.0)) - 5.0) > 0.0001) return 10;
    for (uint seed = 0; seed < 64; ++seed)
        for (int step = -100; step <= 100; ++step)
            if (length(leafLocalDisplacement(seed, float(step) * 0.1, 5.0)) > 5.00001) return 11;
    if (length(leafLocalDisplacement(11u, 0.4, 5.0)) < 10.0 * length(local)) return 12;
    return 0;
}
