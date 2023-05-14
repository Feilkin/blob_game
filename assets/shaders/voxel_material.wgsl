// forward pass material rendering for voxels

#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::pbr_bindings
#import bevy_pbr::mesh_bindings

#import bevy_pbr::utils
#import bevy_pbr::clustered_forward
#import bevy_pbr::lighting
#import bevy_pbr::pbr_ambient
#import bevy_pbr::shadows
#import bevy_pbr::fog
#import bevy_pbr::pbr_functions

#import bevy_pbr::prepass_utils

#import "shaders/raymarching_common.wgsl"


struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32
}

@fragment
fn fragment(
    @builtin(position) fragment_position: vec4<f32>,
    @builtin(sample_index) sample_index: u32,
    #import bevy_pbr::mesh_vertex_output
) -> FragmentOutput {
    var out: FragmentOutput;

    let ray_direction = set_up_ray(fragment_position);
    let ray_origin = view.world_position;

    let prepass_depth_v = prepass_depth(fragment_position, sample_index);
    let prepass_depth_in_world = depth_to_distance(prepass_depth_v, fragment_position.xy);

    let distance_in_world_space = raymarch(ray_origin, ray_direction, prepass_depth_in_world);
    let ray_hit = ray_origin + ray_direction * distance_in_world_space;

    let depth = point_to_depth(ray_hit);

    if (abs(depth - prepass_depth_v) < 0.00000001) {
        discard;
    }

    let normal = calculate_normal(ray_hit);
    let ao = calculate_ao(ray_hit, normal);
    let thickness = 1.0 - calculate_thickness(ray_hit, normal);

    var pbr_input: PbrInput = pbr_input_new();
    pbr_input.material.base_color = vec4(1.0, 0.51, 0.41, 1.0);
    pbr_input.material.emissive = vec4(3.9, 0.1, 0.0, 1.0) * (thickness + 0.1) * 0.3 * (sin(globals.time * 1.61) * 0.4 + 0.6);
    pbr_input.material.reflectance = 0.6;
    pbr_input.material.perceptual_roughness = 0.17;
    pbr_input.material.metallic = 0.3;
    pbr_input.occlusion = ao;
    pbr_input.frag_coord = fragment_position;
    pbr_input.world_position = vec4(ray_hit, 1.0);
    pbr_input.world_normal = normal;
    pbr_input.N = normal;
    pbr_input.V = -ray_direction;

    out.color = pbr(pbr_input);
    out.depth = depth;

    return out;
}