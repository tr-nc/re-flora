// Shared root-fixed deformation. Every attached surface uses this same field.
module rooted_plant_motion;
public float3 rootedPlantOffset(float localY,float height,float3 tipOffset) {
    float h=saturate(localY/max(height,1e-7));
    return tipOffset*h*h;
}
public float3 rootedPlantNormal(float3 normal,float3 up,float localY,float height,float scale,float3 tipOffset) {
    float h=saturate(localY/max(height,1e-7));
    return normalize(normal-up*dot(normal,tipOffset)*(2.0*h/max(height*scale,1e-7)));
}
