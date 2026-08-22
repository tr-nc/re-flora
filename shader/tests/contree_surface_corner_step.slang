import contree_marching;

bool isSingleAxisMask(bool3 mask)
{
    return uint(mask.x) + uint(mask.y) + uint(mask.z) == 1u;
}

export __extern_cpp int main()
{
    bool3 xyTie = contreeAdvanceSideMask(float3(1.0, 1.0, 2.0));
    bool3 yzTie = contreeAdvanceSideMask(float3(2.0, 1.0, 1.0));
    bool3 xyzTie = contreeAdvanceSideMask(float3(1.0));
    bool3 zMinimum = contreeAdvanceSideMask(float3(3.0, 2.0, 1.0));

    printf("xy=%u,%u,%u yz=%u,%u,%u xyz=%u,%u,%u z=%u,%u,%u\n",
           uint(xyTie.x), uint(xyTie.y), uint(xyTie.z),
           uint(yzTie.x), uint(yzTie.y), uint(yzTie.z),
           uint(xyzTie.x), uint(xyzTie.y), uint(xyzTie.z),
           uint(zMinimum.x), uint(zMinimum.y), uint(zMinimum.z));

    // The contree stores surface voxels, not filled interior voxels. At an
    // exact edge or corner, crossing every tied axis at once can jump over
    // all face-adjacent surface cells and land directly in an unrepresented
    // interior cell. Advance one axis per iteration so the surface remains
    // watertight.
    bool oneAxisPerStep = isSingleAxisMask(xyTie) &&
        isSingleAxisMask(yzTie) && isSingleAxisMask(xyzTie) &&
        isSingleAxisMask(zMinimum);
    bool stableTieBreak = all(xyTie == bool3(true, false, false)) &&
        all(yzTie == bool3(false, true, false)) &&
        all(xyzTie == bool3(true, false, false)) &&
        all(zMinimum == bool3(false, false, true));
    return oneAxisPerStep && stableTieBreak ? 0 : 1;
}
