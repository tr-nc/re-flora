// Model-anchored perspective sample lattice. Only its orientation follows the
// current camera continuously: no atlas, nearest-view key, or angular bins.
module flower_stem_model_sampling;

public struct StemModelGrid {
    public float3 center;
    public float3 x;
    public float3 y;
    public float3 normal; // camera -> center
    public float cell;
};
public StemModelGrid stemModelGrid(float3 center,float radius,float height,uint resolution,
    float3 camera,float3 cameraRight) {
    StemModelGrid grid;
    grid.center=center;grid.x=float3(1,0,0);grid.y=float3(0,1,0);
    grid.normal=float3(0,0,-1);grid.cell=0.0;
    float3 delta=center-camera;
    float distance=length(delta);
    if(distance<=1e-7)return grid;
    grid.normal=delta/distance;
    float3 x=cameraRight-grid.normal*dot(cameraRight,grid.normal);
    float lengthX=length(x);
    // At the edge-on camera plane there is no usable perspective lattice.
    // Fade its cell size rather than selecting a different discrete axis.
    if(lengthX<=1e-7)return grid;
    grid.x=x/lengthX;grid.y=cross(grid.x,grid.normal);
    float baseCell=height/float(max(resolution,1u));
    // Only inside the bounding sphere refine toward continuous sampling. This
    // avoids the center singularity and bounds perspective fringe by 2*baseCell.
    // Outside: exactly fixed model-unit cells, independent of camera distance.
    grid.cell=baseCell*min(1.0,distance/max(radius,1e-7))*smoothstep(0.0,0.1,lengthX);
    return grid;
}
public float3 stemModelSampleRay(StemModelGrid grid,float3 camera,float3 displayRay) {
    float facing=dot(displayRay,grid.normal);
    if(grid.cell<=1e-8||facing<=1e-6)return displayRay;
    float distance=dot(grid.center-camera,grid.normal)/facing;
    float3 point=camera+displayRay*distance-grid.center;
    float2 uv=float2(dot(point,grid.x),dot(point,grid.y));
    // Cell centers include the plant center; no sliding screen-space origin.
    float2 cell=floor(uv/grid.cell+0.5)*grid.cell;
    float3 samplePoint=grid.center+grid.x*cell.x+grid.y*cell.y;
    return normalize(samplePoint-camera);
}
public float3 stemModelDisplayPoint(StemModelGrid grid,float3 camera,float3 displayRay,float3 hit) {
    float facing=dot(displayRay,grid.normal);
    if(grid.cell<=1e-8||facing<=1e-6)return hit;
    // Perspective depth of the sampled surface, not a pasted sprite depth.
    return camera+displayRay*(dot(hit-camera,grid.normal)/facing);
}
