import numpy as np
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d import Axes3D
from mpl_toolkits.mplot3d.art3d import Poly3DCollection

def visualize_sphere_culling(bound=2, radius=2.4):
    """
    Visualize which voxels are kept or culled based on a sphere test.
    
    Parameters:
    bound (int): The bound of the cube (from -bound to bound inclusive)
    radius (float): The radius of the sphere used for culling
    """
    # Create figure
    fig = plt.figure(figsize=(10, 8))
    ax = fig.add_subplot(111, projection='3d')
    
    # Lists to store coordinates
    kept_voxels = []
    culled_voxels = []
    
    # Check each voxel in the cube
    for i in range(-bound, bound+1):
        for j in range(-bound, bound+1):
            for k in range(-bound, bound+1):
                # Check if the voxel is inside the sphere
                if i*i + j*j + k*k <= radius*radius:
                    # Keep this voxel
                    kept_voxels.append((i, j, k))
                else:
                    # Cull this voxel
                    culled_voxels.append((i, j, k))
    
    # Function to create cube vertices
    def create_cube(pos, size=0.9):
        x, y, z = pos
        half = size / 2
        
        # Define the 8 vertices of the cube
        vertices = [
            [x - half, y - half, z - half],
            [x + half, y - half, z - half],
            [x + half, y + half, z - half],
            [x - half, y + half, z - half],
            [x - half, y - half, z + half],
            [x + half, y - half, z + half],
            [x + half, y + half, z + half],
            [x - half, y + half, z + half]
        ]
        
        # Define the 6 faces of the cube using the vertices
        faces = [
            [vertices[0], vertices[1], vertices[2], vertices[3]],  # bottom
            [vertices[4], vertices[5], vertices[6], vertices[7]],  # top
            [vertices[0], vertices[1], vertices[5], vertices[4]],  # front
            [vertices[1], vertices[2], vertices[6], vertices[5]],  # right
            [vertices[2], vertices[3], vertices[7], vertices[6]],  # back
            [vertices[3], vertices[0], vertices[4], vertices[7]]   # left
        ]
        
        return faces
    
    # Draw kept voxels (blue)
    for pos in kept_voxels:
        faces = create_cube(pos)
        cube = Poly3DCollection(faces, alpha=0.5, linewidths=1, edgecolors='black')
        cube.set_facecolor('blue')
        ax.add_collection3d(cube)
    
    # Draw culled voxels (red, more transparent)
    for pos in culled_voxels:
        faces = create_cube(pos)
        cube = Poly3DCollection(faces, alpha=0.2, linewidths=1, edgecolors='black')
        cube.set_facecolor('red')
        ax.add_collection3d(cube)
    
    # Add sphere wireframe for reference
    u, v = np.mgrid[0:2*np.pi:20j, 0:np.pi:10j]
    x = radius * np.cos(u) * np.sin(v)
    y = radius * np.sin(u) * np.sin(v)
    z = radius * np.cos(v)
    ax.plot_wireframe(x, y, z, color="green", alpha=0.4)
    
    # Create custom legend elements
    from matplotlib.patches import Patch
    legend_elements = [
        Patch(facecolor='blue', edgecolor='black', alpha=0.5, label='Kept Voxels'),
        Patch(facecolor='red', edgecolor='black', alpha=0.2, label='Culled Voxels')
    ]
    ax.legend(handles=legend_elements, loc='upper right')
    
    # Set labels and title
    ax.set_xlabel('X')
    ax.set_ylabel('Y')
    ax.set_zlabel('Z')
    ax.set_title(f'Voxel Culling with Sphere (radius={radius})')
    
    # Set axis limits
    max_val = max(bound, radius) + 1
    ax.set_xlim(-max_val, max_val)
    ax.set_ylim(-max_val, max_val)
    ax.set_zlim(-max_val, max_val)
    
    # Set equal aspect ratio to ensure cubes look cubic
    ax.set_box_aspect([1, 1, 1])
    
    # Add a note about voxel count
    kept_count = len(kept_voxels)
    culled_count = len(culled_voxels)
    total_count = kept_count + culled_count
    ax.text2D(0.05, 0.95, f"Kept: {kept_count}/{total_count} ({kept_count/total_count:.1%})\nCulled: {culled_count}/{total_count} ({culled_count/total_count:.1%})", 
             transform=ax.transAxes, fontsize=10, bbox=dict(facecolor='white', alpha=0.7))
    
    # Print GLSL array format of kept voxels
    print("\nGLSL ivec3 array of kept voxels:")
    print("ivec3 offsets[] = {")
    
    # Format each voxel as "ivec3(x, y, z),"
    formatted_voxels = []
    for i, (x, y, z) in enumerate(kept_voxels):
        formatted_voxels.append(f"    ivec3({x}, {y}, {z})")
    
    # Join with commas and add trailing comma to all but the last one
    glsl_array = ",\n".join(formatted_voxels)
    print(f"{glsl_array}")
    print("};")
    
    # Also print the count for array size declaration if needed
    print(f"\n// Total count: {kept_count}")
    print(f"// Use in shader: const int OFFSET_COUNT = {kept_count};")
    
    plt.tight_layout()
    plt.show()
    
    return kept_voxels, culled_voxels

# Example usage with default parameters (bound=2, radius=2)
kept, culled = visualize_sphere_culling()

# You can also try with different parameters
# kept, culled = visualize_sphere_culling(bound=3, radius=2.5)
