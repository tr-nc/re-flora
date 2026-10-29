module flora_brush;

// One brush description shared by local surface extraction and occupancy edits.
public struct U_EditOccupancyInfo
{
    public float4 edit_segment_start_radius_vox;
    public float4 edit_segment_end_radius_vox;
    public uint3 chunk_world_offset;
    public uint3 chunk_dim;
    public uint mode;
    public uint flora_tick;
    public uint target_age;
    public uint paint_selection;
    public uint3 surface_offset;
    public uint3 surface_dim;
};
