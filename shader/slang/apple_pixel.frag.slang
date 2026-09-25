import apple_pixel_tile;
import apple_pixel_frame;
[shader("fragment")]
ApplePixelOutput main(ApplePixelVertex input) {
    return sampleApplePixel(input);
}
