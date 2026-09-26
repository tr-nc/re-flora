// One attached-leaf pose contract for model drawing AND detachment handoff.
// A holds a deterministic world-space initial attitude. B adds the existing
// leaf hinge response. Both follow the branch's position; no second wind solver.
import flora_vertex;
import flora_types;
import vegetation_response_sample;
import tree_scene;
import leaf_flutter;

public float4 leafQuaternionProduct(float4 a,float4 b) {
    return float4(a.w*b.xyz+b.w*a.xyz+cross(a.xyz,b.xyz),a.w*b.w-dot(a.xyz,b.xyz));
}
public float3 leafQuaternionVector(float4 q,float3 v) {
    return v+2.0*cross(q.xyz,cross(q.xyz,v)+q.w*v);
}
public float4 leafInitialRotation(uint seed) {
    float3 n=leafRestNormal(seed);
    float4 align=normalize(float4(cross(float3(0,0,1),n),1.0+n.z));
    float halfRoll=leafUnit(seed^0x68bc21ebu)*3.14159265359;
    return leafQuaternionProduct(align,float4(0,0,sin(halfRoll),cos(halfRoll)));
}
public struct LeafModelPose {
    public float3 center;
    public float size; // physical leaf size; shared display scale is applied only by rendering
    public float4 rotation;
    public float4 color;
    public uint seed;
};
public LeafModelPose attachedLeafModelPose(uint3 world,uint packedLocal,uint index,float growth) {
    int3 local=unpackTreeLeafVoxelLocalPosition(packedLocal);
    uint3 anchor=uint3(int3(world)-local);
    uint seed=instanceSeed(anchor);
    uint info=lookupFloraVoxelInfo(FLORA_SPECIES_TREE_LEAF,local);
    PreparedFloraVertex p=prepareFloraVertex(local,info,anchor,
        FLORA_SPECIES_TREE_LEAF,seed,INSTANCE_GROWTH_PROGRESS_MATURE,
        1.0,1.0,INSTANCE_SPAWN_INACTIVE,index);
    LeafModelPose leaf;
    leaf.seed=instanceSeed(world);
    leaf.center=p.voxel_position;
    leaf.size=growth/256.0;
    leaf.color=float4(sampleFloraBaseColor(false,FLORA_SPECIES_TREE_LEAF,
        seed,local,anchor,p.color_gradient,info),p.should_trim?0.0:1.0);
    leaf.rotation=leafInitialRotation(leaf.seed);
    if(gui_input.attached_leaf_rotation!=0u && attachedVegetationResponseEnabled()) {
        float angle=sampleLeafAngle(leaf.seed,index,floraPushConstants().response_offset);
        float halfAngle=angle*0.5;
        leaf.rotation=normalize(leafQuaternionProduct(
            float4(leafHinge(leaf.seed)*sin(halfAngle),cos(halfAngle)),leaf.rotation));
    }
    return leaf;
}
