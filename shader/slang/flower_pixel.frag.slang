import flower_pixel_types;
import model_pixel_display;
[shader("fragment")]
ModelPixelOutput main(FlowerPixelVertex input) {
    return displayModelPixel(input.uv,input.resolution,input.offset,1.0,input.center,input.radius);
}
