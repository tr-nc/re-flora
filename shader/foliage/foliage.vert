#version 450

// No 'in' variables are needed! We generate everything from gl_VertexIndex.

// We can still pass data to the fragment shader if we want, e.g., for UVs.
// For this simple example, we don't need to.

void main() {
    // This is a common trick to generate a full-screen triangle.
    // The vertices are intentionally oversized to ensure full coverage
    // of the [-1, 1] Normalized Device Coordinate (NDC) space.
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2( 0.0, 1.0),
        vec2(1.0,  -1.0)
    );

    // Select the position from the array based on the vertex being processed.
    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
    
    // test commit...
}