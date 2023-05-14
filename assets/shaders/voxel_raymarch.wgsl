// depth pass raymarching for voxels

#import bevy_pbr::prepass_bindings
#import "shaders/raymarching_common.wgsl"

struct FragmentOutput {
    @location(0) normal: vec4<f32>,
    @builtin(frag_depth) depth: f32
}


@fragment
fn fragment(
    @builtin(position) fragment_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
) -> FragmentOutput {
    var out: FragmentOutput;

    let ray_direction = set_up_ray(fragment_position);
    let ray_origin = view.world_position.xyz;

    let distance_in_world_space = raymarch(ray_origin, ray_direction, 1000.);

    if (distance_in_world_space >= 1000.) {
        discard;
    }

    let ray_hit = ray_origin + ray_direction * distance_in_world_space;

    let normal = calculate_normal(ray_hit, distance_in_world_space);
    let surface_depth = point_to_depth(ray_hit);

    out.depth = surface_depth;
    out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
    return out;
}