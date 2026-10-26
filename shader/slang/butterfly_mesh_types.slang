public struct ButterflyMeshInstance {
    public float4 position_size;
    public float4 color;
    public uint4 metadata; // source, reserved, resolution, flags (bit 1: leaf; bit 2: firefly; bit 3: bee; bit 4: hive)
    public float4 lighting; // x: wing transmission; y: firefly emission pulse
    public uint4 tile; // offset, visible, reserved, reserved
    public float4 view_orientation;
    public uint4 flower_attachment; // current-frame pose slot, enabled, yaw bits, clearance bits
};
