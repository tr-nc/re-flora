import glass_optics;
import voxel_types;

bool closeDirection(float3 a, float3 b)
{
    return length(a - b) < 2.0e-5;
}

export __extern_cpp int main()
{
    int failures = 0;
    if (!glassHasOpticalBoundary(VOXEL_TYPE_GLASS, VOXEL_TYPE_EMPTY, true) ||
        glassHasOpticalBoundary(VOXEL_TYPE_GLASS, VOXEL_TYPE_GLASS, true) ||
        glassHasOpticalBoundary(VOXEL_TYPE_GLASS, VOXEL_TYPE_OAK_WOOD, true) ||
        glassHasOpticalBoundary(VOXEL_TYPE_GLASS, VOXEL_TYPE_ROCK, true) ||
        !glassHasOpticalBoundary(VOXEL_TYPE_GLASS, VOXEL_TYPE_EMPTY, false) ||
        !glassHasOpticalBoundary(VOXEL_TYPE_SAND, VOXEL_TYPE_EMPTY, true) ||
        glassHasOpticalBoundary(VOXEL_TYPE_SAND, VOXEL_TYPE_EMPTY, false))
    {
        printf("opaque frame or same-medium cell became an optical boundary\n");
        ++failures;
    }
    float3 frontNormal = float3(0, 0, 1);
    // Include actual rim normals captured from the cottage's two-voxel panes.
    float3 appearances[5] = {
        frontNormal, normalize(float3(0.9859006, 0, 0.1673318)),
        normalize(float3(0, 0.8345468, 0.5509100)),
        float3(1, 0, 0), normalize(float3(-0.7, -0.7, 0.2))
    };
    for (int side = 0; side < 2; ++side)
    for (int angle = -75; angle <= 75; angle += 15)
    for (int appearance = 0; appearance < 5; ++appearance)
    {
        float radians = float(angle) * 0.0174532925199433;
        float3 incident = side == 0
            ? float3(sin(radians), 0, -cos(radians))
            : float3(0, sin(radians), -cos(radians));
        GlassInterfaceScatter entry = scatterGlassInterface(
            incident, frontNormal, appearances[appearance], 1.0, 1.5);
        GlassInterfaceScatter exit = scatterGlassInterface(
            entry.transmitted, frontNormal, frontNormal, 1.5, 1.0);
        // Appearance normals may move highlights, not invent a wedge in a
        // parallel slab or turn a valid outside->outside path into trapped TIR.
        if (!entry.has_transmission || !exit.has_transmission ||
            !closeDirection(exit.transmitted, incident))
        {
            printf("parallel slab failed side=%d angle=%d appearance=%d entry=%d exit=%d\n",
                side, angle, appearance, int(entry.has_transmission), int(exit.has_transmission));
            ++failures;
        }
        if (!(dot(entry.reflected, frontNormal) > 0.0) ||
            !(dot(entry.transmitted, frontNormal) < 0.0) ||
            any(isnan(entry.reflected)) || any(isinf(entry.reflected)) ||
            abs(length(entry.reflected) - 1.0) > 1e-5)
        {
            printf("wrong interface hemisphere side=%d angle=%d appearance=%d\n",
                side, angle, appearance);
            ++failures;
        }
        GlassInterfaceScatter plain = scatterGlassInterface(
            incident, frontNormal, frontNormal, 1.0, 1.5);
        if (abs(entry.fresnel - plain.fresnel) > 1e-6)
        {
            printf("appearance changed boundary energy\n");
            ++failures;
        }
    }
    // A genuinely supercritical ray starting inside glass must still undergo TIR.
    GlassInterfaceScatter tir = scatterGlassInterface(
        normalize(float3(1, 0, -0.1)), frontNormal, frontNormal, 1.5, 1.0);
    if (tir.has_transmission || abs(tir.fresnel - 1.0) > 1e-6) ++failures;
    printf("glass interface optics failures=%d\n", failures);
    return failures == 0 ? 0 : 1;
}
