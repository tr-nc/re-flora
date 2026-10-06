import grass_growth_shape;

export __extern_cpp int main()
{
    // Both species, including randomly shortened stems, reach the tip palette
    // at their competition/environment-limited mature height.
    for (uint height = 2u; height <= 8u; ++height)
    {
        for (uint step = 1u; step <= 20u; ++step)
        {
            float potential = float(step) / 20.0;
            int matureHeight = int(grassGrownHeight(height, potential));
            if (matureHeight == 0) continue;
            if (abs(grassColorGradient(matureHeight - 1, height, potential) - 1.0) > 0.0001)
                return 1;
            if (matureHeight > 1 && grassColorGradient(0, height, potential) != 0.0)
                return 2;
            // Growth has not reached its local maximum: do not promote the
            // current tip to the final palette prematurely.
            if (matureHeight > 2 && grassColorGradient(matureHeight - 2, height, potential) >= 1.0)
                return 3;
        }
    }
    if (grassGrownHeight(8u, 0.5) != 4.0) return 4;
    if (grassColorGradient(3, 8u, 0.5) != 1.0) return 5;
    if (abs(grassColorGradient(1, 8u, 0.5) - 1.0 / 3.0) > 0.0001) return 6;
    return 0;
}
