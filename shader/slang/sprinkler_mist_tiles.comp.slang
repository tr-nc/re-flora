import sprinkler_mist_transport;

[[vk::binding(2,1)]] StructuredBuffer<B_SprinklerMistBounds> mist_bounds;
[[vk::binding(3,1)]] RWStructuredBuffer<uint> mist_tiles;
[[vk::push_constant]] ConstantBuffer<PushConstantSprinklerMist> mist_push;

// One word per tile / 32 instances. Every live word is written, no stale bits
// after removal, mode changes or resize; no fixed per-tile overflow capacity.
[shader("compute")]
[numthreads(8,8,1)]
void main(uint3 id:SV_DispatchThreadID)
{
    uint columns=(mist_push.width+mist_push.tile_size-1u)/mist_push.tile_size;
    uint rows=(mist_push.height+mist_push.tile_size-1u)/mist_push.tile_size;
    uint words=(mist_push.count+31u)/32u;
    uint tileX=id.x/words,word=id.x%words;
    if(tileX>=columns || id.y>=rows)return;
    float2 lo=float2(tileX,id.y)*float(mist_push.tile_size)/float2(mist_push.width,mist_push.height);
    float2 hi=float2(tileX+1u,id.y+1u)*float(mist_push.tile_size)/float2(mist_push.width,mist_push.height);
    uint mask=0u;
    for(uint bit=0u;bit<32u;++bit) {
        uint instance=word*32u+bit;
        if(instance>=mist_push.count)break;
        float4 rect=mist_bounds[instance].screen_rect;
        if(all(rect.xy<=rect.zw) && all(rect.xy<=hi) && all(rect.zw>=lo))mask|=1u<<bit;
    }
    mist_tiles[(id.y*columns+tileX)*words+word]=mask;
}
