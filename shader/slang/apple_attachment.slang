module apple_attachment;
import tracer_types;
import tree_scene;

// Matches the authored upper stem cap (apple-source.mjs), in native voxel units.
public static const float3 APPLE_STEM_TIP = float3(0.10574398,2.0463712,0);
public float3 appleRotate(float4 q, float3 v) {
    return v + 2.0*cross(q.xyz,cross(q.xyz,v)+q.w*v);
}
public float4 appleHeldRotation(U_GuiInput gui, float3 response, bool inertial) {
    // One mechanical state feeds visible pose, shadows, picking and release.
    // With response disabled, only the owning branch moves (no second oscillator).
    float magnitude = inertial ? length(response.xz) : 0.0;
    if (magnitude <= 0.000001) return float4(0,0,0,1);
    float angle = magnitude/(1.0+magnitude)*clamp(gui.fruit_swing_max_angle_radians,0.0,1.570796327);
    float3 axis = float3(-response.z,0,response.x)/magnitude;
    return float4(axis*sin(angle*0.5),cos(angle*0.5));
}
public float3 appleAttachedPosition(uint3 root, float scale, float4 rotation, float3 modelVoxel) {
    float3 rest = (treeFruitPivot(root) + appleRotate(rotation,(modelVoxel-APPLE_STEM_TIP)*scale))/256.0;
    return treeAttachmentPoint(treeFruitKey(root),rest);
}
public float3 appleAttachedVector(uint3 root, float4 rotation, float3 vector) {
    return treeAttachmentVector(treeFruitKey(root),appleRotate(rotation,vector));
}
