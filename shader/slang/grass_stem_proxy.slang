module grass_stem_proxy;
import flower_stem_model_sampling;
public struct GrassStemProxyVertex {
    public float4 position:SV_Position;
    [[vk::location(0)]] public noperspective float3 ray:TEXCOORD0;
    [[vk::location(1)]] public nointerpolation float4 root_count:TEXCOORD1;
    [[vk::location(2)]] public nointerpolation float4 grid_center_cell:TEXCOORD2;
    [[vk::location(3)]] public nointerpolation float3 grid_x:TEXCOORD3;
    [[vk::location(4)]] public nointerpolation float3 grid_y:TEXCOORD4;
    [[vk::location(5)]] public nointerpolation float3 points[8]:TEXCOORD5;
    [[vk::location(13)]] public nointerpolation float3 colors[8]:TEXCOORD13;
};
public StemModelGrid grassStemGrid(GrassStemProxyVertex p) {
    StemModelGrid grid;
    grid.center=p.grid_center_cell.xyz;grid.cell=p.grid_center_cell.w;
    grid.x=p.grid_x;grid.y=p.grid_y;grid.normal=cross(grid.y,grid.x);
    return grid;
}
