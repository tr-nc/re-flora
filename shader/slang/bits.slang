module bits;

public uint bitCountDual(uint lowBits, uint highBits)
{
    return countbits(lowBits) + countbits(highBits);
}

public uint bitCountDualBelow(uint lowBits, uint highBits, uint width)
{
    if (width < 32u)
    {
        return countbits(lowBits & ((1u << width) - 1u));
    }
    return countbits(lowBits) + countbits(highBits & ((1u << (width - 32u)) - 1u));
}
