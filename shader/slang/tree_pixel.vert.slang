[shader("vertex")]
float4 main(uint vertex:SV_VulkanVertexID):SV_Position {
    // Common quad index order: 0,1,2,2,1,3. This is display geometry only.
    return float4(float2(vertex&1u,vertex>>1u)*2.0-1.0,0,1);
}
