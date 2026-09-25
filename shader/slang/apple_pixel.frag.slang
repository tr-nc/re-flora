import apple_pixel_frame;
import model_pixel_display;
[shader("fragment")]
ModelPixelOutput main(ApplePixelVertex input) {
    return displayModelPixel(input.uv,input.resolution,
        input.tileIndex*input.resolution*input.resolution,1.0);
}
