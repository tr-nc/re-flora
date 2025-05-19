use glam::Vec3;
use std::f32::consts::PI;

/// Returns a direction vector given
/// - θ (theta): azimuthal angle (radians), measured from +Z toward +X
/// - φ (phi): polar angle (radians), measured from +Y down
fn spherical_dir(theta: f32, phi: f32) -> Vec3 {
    let sin_phi = phi.sin();
    Vec3::new(
        sin_phi * theta.sin(), // x
        phi.cos(),             // y
        sin_phi * theta.cos(), // z
    )
}

/// Returns a unit‐sphere direction from
/// - alt: altitude angle (radians), measured up from the horizon
/// - azi: azimuth angle (radians), measured around the Y axis
fn dir_on_unit_sphere(alt: f32, azi: f32) -> Vec3 {
    // Convert altitude to polar angle φ = π/2 − alt
    spherical_dir(azi, PI * 0.5 - alt)
}

/// Returns the sun direction (unit vector) given
/// - sun_altitude_deg: degrees above the horizon
/// - sun_azimuth_deg:  degrees around the Y axis
pub fn get_sun_dir(sun_altitude_deg: f32, sun_azimuth_deg: f32) -> Vec3 {
    let alt = sun_altitude_deg.to_radians();
    let azi = sun_azimuth_deg.to_radians();
    dir_on_unit_sphere(alt, azi)
}
