module contree_node;

public struct ContreeNode
{
    public uint packed_0;
    public uint child_mask_lo;
    public uint child_mask_hi;
};

public bool childMaskTest(ContreeNode node, uint index)
{
    if (index < 32u)
    {
        return (node.child_mask_lo & (1u << index)) != 0u;
    }
    return (node.child_mask_hi & (1u << (index - 32u))) != 0u;
}

public uint childMaskBitCountBelow(ContreeNode node, uint index)
{
    if (index < 32u)
    {
        return countbits(node.child_mask_lo & ((1u << index) - 1u));
    }
    return countbits(node.child_mask_lo) +
           countbits(node.child_mask_hi & ((1u << (index - 32u)) - 1u));
}

public bool childMaskTestPattern(ContreeNode node, uint index, uint wideMask)
{
    uint shiftedIndex = index & 0x2Au;
    if (shiftedIndex < 32u)
    {
        return ((node.child_mask_lo >> shiftedIndex) & wideMask) != 0u;
    }
    return ((node.child_mask_hi >> (shiftedIndex - 32u)) & wideMask) != 0u;
}
