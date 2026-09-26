module tree_pixel_pass;
public struct TreePixelPass {
    public uint4 view; // actual scene width, height, block size, unused
    public uint4 band; // first grid row, number of rows, unused, unused
};
public uint2 treePixelGrid(TreePixelPass pass) {
    return (pass.view.xy+pass.view.z-1u)/pass.view.z;
}
// Adapt the common square-grid API to a rectangular, integer-aligned screen
// grid. Only the scheduled rows exist; no camera/view quantization is involved.
public float4 treePixelBounds(TreePixelPass pass) {
    float span=2.0*float(treePixelGrid(pass).x*pass.view.z);
    return float4(-1,-1,-1.0+span/float(pass.view.x),-1.0+span/float(pass.view.y));
}
