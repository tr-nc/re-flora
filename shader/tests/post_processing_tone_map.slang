import tone_mapping;

bool nearlyEqual(float a, float b)
{
    return abs(a - b) < 1.0e-5;
}

export __extern_cpp int main()
{
    float black = toneMapDisplayLuminance(0.0);
    float dark = toneMapDisplayLuminance(0.02);
    float middle = toneMapDisplayLuminance(0.5);
    float white = toneMapDisplayLuminance(4.0);

    printf("black=%.9f dark=%.9f middle=%.9f white=%.9f\n",
           black, dark, middle, white);

    bool blackStaysBlack = nearlyEqual(black, 0.0);
    bool darkDetailIsLifted = dark >= 0.038 && dark <= 0.039;
    bool middleGrayStaysNearMiddle = middle >= 0.50 && middle <= 0.52;
    bool sceneWhiteReachesDisplayWhite = nearlyEqual(white, 1.0);
    return blackStaysBlack && darkDetailIsLifted &&
        middleGrayStaysNearMiddle && sceneWhiteReachesDisplayWhite
        ? 0 : 1;
}
