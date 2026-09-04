import contree_marching;

bool isSingleAxisMask(uint mask)
{
    return mask != 0u && (mask & (mask - 1u)) == 0u;
}

export __extern_cpp int main()
{
    uint xyTie = contreeAdvanceSideMask(float3(1.0, 1.0, 2.0));
    uint yzTie = contreeAdvanceSideMask(float3(2.0, 1.0, 1.0));
    uint xyzTie = contreeAdvanceSideMask(float3(1.0));
    uint zMinimum = contreeAdvanceSideMask(float3(3.0, 2.0, 1.0));

    printf("xy=%u yz=%u xyz=%u z=%u\n", xyTie, yzTie, xyzTie, zMinimum);

    // The contree stores surface voxels, not filled interior voxels. At an
    // exact edge or corner, crossing every tied axis at once can jump over
    // all face-adjacent surface cells and land directly in an unrepresented
    // interior cell. Advance one axis per iteration so the surface remains
    // watertight.
    bool oneAxisPerStep = isSingleAxisMask(xyTie) &&
        isSingleAxisMask(yzTie) && isSingleAxisMask(xyzTie) &&
        isSingleAxisMask(zMinimum);
    bool stableTieBreak = xyTie == 1u && yzTie == 2u &&
        xyzTie == 1u && zMinimum == 4u;
    return oneAxisPerStep && stableTieBreak ? 0 : 1;
}
