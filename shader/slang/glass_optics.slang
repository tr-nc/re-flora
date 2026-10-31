module glass_optics;

import scene_query;
import voxel_material;

// A glass cell next to a frame still has an exposed pane face. The opaque
// contact is a terminal, not a camera-visible refractive entry/exit interface.
public bool glassHasOpticalBoundary(uint glassType, uint neighborType, bool enabled)
{
    VoxelMaterial glass = voxelMaterial(glassType, enabled);
    VoxelMaterial neighbor = voxelMaterial(neighborType, enabled);
    return glass.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC &&
        neighbor.surface_class != VOXEL_SURFACE_CLASS_OPAQUE &&
        !(neighbor.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC &&
          glassType == neighborType);
}

public struct GlassInterfaceScatter
{
    public float3 reflected;
    public float3 transmitted;
    public float fresnel;
    public bool has_transmission;
};

public GlassInterfaceScatter scatterGlassInterface(
    float3 incident, float3 geometricNormal, float3 appearanceNormal,
    float fromIor, float toIor)
{
    GlassInterfaceScatter result;
    // Snell/Fresnel describe the actual medium boundary. An occupancy-derived
    // appearance normal is not a second, tilted entry plane: using it only on
    // entry invents a prism and traps rays against the axis-aligned exit face.
    result.fresnel = sceneQueryDielectricFresnel(
        incident, geometricNormal, fromIor, toIor);
    result.has_transmission = sceneQueryRefractDielectric(
        incident, geometricNormal, fromIor, toIor, result.transmitted);

    // Retain the optional stored-normal highlight without reflecting through
    // the geometric boundary into the transmitted hemisphere.
    result.reflected = normalize(reflect(incident, appearanceNormal));
    if (dot(incident, appearanceNormal) >= 0.0 ||
        dot(result.reflected, geometricNormal) <= 0.0)
        result.reflected = normalize(reflect(incident, geometricNormal));
    return result;
}
