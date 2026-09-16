module tree_scene;
import marching_result;
import tree_cells;
struct TreeSceneNode { float3 lower; uint escape; float3 upper; uint triangle; };
[[vk::binding(49,0)]] StructuredBuffer<uint4> tree_scene_info;
[[vk::binding(50,0)]] StructuredBuffer<TreeSceneNode> tree_scene_nodes;
[[vk::binding(51,0)]] StructuredBuffer<uint4> tree_scene_triangles;
[[vk::binding(52,0)]] StructuredBuffer<float4> tree_scene_vertices;
[[vk::binding(53,0)]] StructuredBuffer<uint> tree_scene_cell_vertices;
public bool treeSceneActive() { return tree_scene_info[0].x != 0u; }
public float3 treeScenePosition(uint i) { return tree_scene_vertices[i*2u].xyz; }
public float3 treeSceneNormal(uint i) { return tree_scene_vertices[i*2u+1u].xyz; }
public void treeCellReceiver(uint slot, inout float3 center, inout float3 normal) {
    if (!treeSceneActive()) return;
    uint base = tree_scene_cell_vertices[slot];
    center = float3(0); normal = float3(0);
    for (uint i=0u;i<8u;++i) { center += treeScenePosition(base+i); normal += treeSceneNormal(base+i); }
    center *= 0.125; normal = normalize(normal);
}
bool intersectsTreeNode(TreeSceneNode node, float3 origin, float3 direction, float limit) {
    float lo = 0.0, hi = limit;
    for (uint axis=0u;axis<3u;++axis) {
        if (abs(direction[axis]) < 1e-10) {
            if (origin[axis] < node.lower[axis] || origin[axis] > node.upper[axis]) return false;
        } else {
            float a = (node.lower[axis]-origin[axis])/direction[axis];
            float b = (node.upper[axis]-origin[axis])/direction[axis];
            lo = max(lo,min(a,b)); hi = min(hi,max(a,b));
        }
    }
    return lo <= hi;
}
public MarchingResult traceTreeScene(float3 origin, float3 direction, MarchingResult hit) {
    if (!treeSceneActive()) return hit;
    uint count = tree_scene_info[0].y;
    uint index = 0u;
    float limit = hit.is_hit ? hit.distance : 1e10;
    while (index < count) {
        TreeSceneNode node = tree_scene_nodes[index];
        if (!intersectsTreeNode(node,origin,direction,limit)) { index = node.escape; continue; }
        if (node.triangle == 0xffffffffu) { ++index; continue; }
        uint3 ids = tree_scene_triangles[node.triangle].xyz;
        float3 a = treeScenePosition(ids.x), b = treeScenePosition(ids.y), c = treeScenePosition(ids.z);
        float3 e1 = b-a, e2 = c-a;
        float3 p = cross(direction,e2);
        float determinant = dot(e1,p);
        if (abs(determinant) > 1e-12) {
            float3 s = origin-a, q = cross(s,e1);
            float u = dot(s,p)/determinant, v = dot(direction,q)/determinant;
            float distance = dot(e2,q)/determinant;
            if (u >= -1e-6 && v >= -1e-6 && u+v <= 1.000001 && distance >= 1e-6 && distance < limit) {
                limit = distance;
                hit.is_hit = true; hit.distance = distance;
                hit.position = origin+direction*distance; hit.center_position = hit.position;
                hit.normal = normalize(treeSceneNormal(ids.x)*(1.0-u-v)+treeSceneNormal(ids.y)*u+treeSceneNormal(ids.z)*v);
                hit.voxel_type = 5u; hit.voxel_address = 0u;
            }
        }
        index = node.escape;
    }
    return hit;
}

[[vk::binding(54,0)]] StructuredBuffer<uint4> tree_attachment_keys;
[[vk::binding(55,0)]] StructuredBuffer<float4> tree_attachment_poses;
uint treeAttachmentIndex(uint3 anchor) {
    uint hash = anchor.x*73856093u ^ anchor.y*19349663u ^ anchor.z*83492791u;
    hash ^= hash>>16u;
    uint count,stride; tree_attachment_keys.GetDimensions(count,stride);
    for (uint i=0u;i<64u;++i) {
        uint4 key=tree_attachment_keys[(hash+i)&(count-1u)];
        if (key.w==0u) return 0u;
        if (all(key.xyz==anchor)) return key.w;
    }
    return 0u;
}
float3 rotateTreeVector(float4 q, float3 v) { return v + 2.0*cross(q.xyz,cross(q.xyz,v)+q.w*v); }
public float3 treeAttachmentPoint(uint3 anchor, float3 position) {
    if (!treeSceneActive()) return position;
    uint index=treeAttachmentIndex(anchor);
    if (index==0u) return position;
    return rotateTreeVector(tree_attachment_poses[(index-1u)*2u],position)+tree_attachment_poses[(index-1u)*2u+1u].xyz;
}
public float3 treeAttachmentVector(uint3 anchor, float3 vector) {
    if (!treeSceneActive()) return vector;
    uint index=treeAttachmentIndex(anchor);
    return index==0u ? vector : rotateTreeVector(tree_attachment_poses[(index-1u)*2u],vector);
}

[[vk::binding(56,0)]] StructuredBuffer<uint4> tree_scene_rest_cells;
public bool treeRestVoxel(uint3 coordinate) {
    if (!treeSceneActive()) return false;
    uint hash=coordinate.x*73856093u ^ coordinate.y*19349663u ^ coordinate.z*83492791u;
    hash ^= hash>>16u;
    uint count,stride; tree_scene_rest_cells.GetDimensions(count,stride);
    for (uint i=0u;i<64u;++i) {
        uint4 cell=tree_scene_rest_cells[(hash+i)&(count-1u)];
        if (cell.w==0u) return false;
        if (all(cell.xyz==coordinate)) return true;
    }
    return false;
}
public bool treeSceneOccludes(float3 start, float3 end) {
    if (!treeSceneActive()) return false;
    float distance=length(end-start);
    if (distance<=1e-6) return false;
    MarchingResult empty=(MarchingResult)0;
    MarchingResult hit=traceTreeScene(start,(end-start)/distance,empty);
    return hit.is_hit && hit.distance < distance-1e-5;
}
