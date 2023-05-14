const MAX_RT_STEPS = 64;
const RT_EPSILON = 0.01f;

struct BlobEntity {
    position: vec2<f32>,
    size: f32,
    direction: f32,
    last_ate: f32,
    color: vec3<f32>,
}

struct BlobData {
    blob_count: u32,
    blobs: array<BlobEntity, 64>,
}

struct BvhNode {
    /// Minimum of the AABB
    min: vec3<f32>,
    /// Maximum of the AABB
    max: vec3<f32>,
    /// Left child index, or -1 if leaf node
    left: i32,
    /// Right child index, or entity index if leaf node
    right: i32,
}

struct BvhTree {
    tree: array<BvhNode>
}

struct HitEntities {
    count: u32,
    entities: array<BlobEntity, 10>,
}

var<private> hit_entities: HitEntities;

@group(1) @binding(0) var<uniform> blob_data: BlobData;
@group(1) @binding(1) var<storage> bvh: BvhTree;

fn opSmoothUnion(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5*(d2-d1)/k, 0.0, 1.0);
    return mix(d2, d1, h) - k*h*(1.0-h);
}
fn opSmoothIntersection(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5*(d2-d1)/k, 0.0, 1.0);
    return mix(d2, d1, h) + k*h*(1.0-h);
}

fn sdf_rounded_cylinder(p: vec3<f32>, ra: f32, rb: f32, h: f32) -> f32 {
    let d = vec2( length(p.xy) - 2.0 * ra+rb, abs(p.z) - h );
    return min(max(d.x,d.y),0.0) + length(max(d,vec2(0.0))) - rb;
}

fn petri_dish(ray_position: vec3<f32>) -> f32 {
    return sdf_rounded_cylinder(ray_position - vec3(0., 0., 10.257), 4.99, 0.1, 10.);
}

fn rotate_z(p: vec3<f32>, r: f32) -> vec3<f32> {
    return vec3(
       p.x * cos(r) - p.y * sin(r),
       p.x * sin(r) + p.y * cos(r),
       p.z
   );
}

fn rotate_x(p: vec3<f32>, r: f32) -> vec3<f32> {
    return vec3(
       p.x,
       p.y * cos(r) - p.z * sin(r),
       p.y * sin(r) + p.z * cos(r),
   );
}

fn ease_out(x: f32) -> f32 {
    return pow(2., -10. * x)*sin((x * 10. - 0.75) * (2. * 3.1415) / 3.) + 1.;
}

fn sdf_blob(ray_position: vec3<f32>, blob: BlobEntity, index: f32) -> f32 {
        let t = 0.7 + sin(globals.time + index) * 0.3;
        let t2 = 15.0 * pow(abs(t), 0.5) * sign(t);
        let ray_local = ray_position - vec3(blob.position, 0.4);
        let ray_rotated = rotate_x(rotate_z(ray_local, -blob.direction), -globals.time);
        var displacement = sin(t2 * ray_rotated.x) * sin(t2 * ray_rotated.y) * sin(t2 * ray_rotated.z);
        let blob_size = blob.size * ease_out(globals.time - blob.last_ate);
        let distance_local = length(ray_rotated) - blob_size * (sin(globals.time * 2.54) * 0.1 + 0.9) + displacement * 0.06;

        return distance_local;
}

fn sdf(ray_position: vec3<f32>) -> f32 {
    var acc = 9000.0;

    for (var i = 0u; i < hit_entities.count; i++) {
        let blob = hit_entities.entities[i];
        acc = opSmoothUnion(acc, sdf_blob(ray_position, blob, 0.0), 0.6);
    }

    let petri = -petri_dish(ray_position);
    acc = opSmoothUnion(acc, petri, 0.4);
    acc = max(acc, petri_dish((ray_position - vec3(0., 0., 0.10)) / 0.99) * 0.99);
//    acc = opSmoothIntersection(acc, -petri, 0.3);

    return acc;
}

fn set_up_ray(fragment_position: vec4<f32>) -> vec3<f32> {
    let fragment_ndc = vec2(fragment_position.x / view.viewport.z, fragment_position.y / view.viewport.w);
    let aspect_ratio = vec2(1.0, -1.0);
    let fragment_normalized = 2.0 * fragment_ndc - 1.0;
    let ray = vec4(fragment_normalized * aspect_ratio, -1.0f, 1.0f);

    let ray_projected = view.inverse_projection * ray;
    let ray_in_world = view.view * (vec4(ray_projected.xyz, 0.0));
    return normalize(ray_in_world.xyz);
}


fn raymarch(ray_origin: vec3<f32>, ray_direction: vec3<f32>, max_distance: f32) -> f32 {
    bvh_lookup_ray(ray_origin, ray_direction);
    var ray_position = ray_origin;
    var distance_acc = 0.0;

    for (var i = 0; i < MAX_RT_STEPS; i++) {
        let closest_surface = sdf(ray_position);

        if (closest_surface <= RT_EPSILON) {
            return distance_acc + closest_surface;
        }

        ray_position += ray_direction * closest_surface;
        distance_acc += closest_surface;

        if (distance_acc >= max_distance) {
            return max_distance;
        }
    }

    return max_distance;
}

fn point_to_depth(position: vec3<f32>) -> f32 {
    let pos_in_clip_space = view.view_proj * vec4(position, 1.0);
    let depth_in_fb = (pos_in_clip_space.z / pos_in_clip_space.w);
    return depth_in_fb;
}

fn depth_to_distance(depth: f32, fragment_position: vec2<f32>) -> f32 {
    let z = depth;// * 2.0 - 1.0;
    let fragment_ndc = vec2(fragment_position.x / view.viewport.z, fragment_position.y / view.viewport.w);
    let fragment_normalized = 2.0 * fragment_ndc - 1.0;
    let view_position = view.inverse_projection * vec4(fragment_normalized.xy, z, 1.0);
    let pos_in_world_space = view.view * vec4(view_position.xyz / view_position.w, 1.0);
    return distance(view.world_position.xyz, pos_in_world_space.xyz);
}

fn calculate_normal(pos: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(1.0,-1.0)*0.57734231*0.0003;
    return normalize( e.xyy * (sdf( pos + e.xyy)) +
                      e.yyx * (sdf( pos + e.yyx)) +
                      e.yxy * (sdf( pos + e.yxy)) +
                      e.xxx * sdf( pos + e.xxx) );
}

fn calculate_ao(pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    var occ = 0.;
    var sca = 1.0;
    for (var i = 0; i < 5; i += 1) {
        let h = 0.01 + 0.12 * f32(i)/4.0;
        let d = sdf(pos + h*normal);
        occ += (h-d) * sca;
        sca *= 0.95;
        if (occ > 0.35) { break; }
    }
    return clamp(1.0 - 3.0*occ, 0.0, 1.0);
}

fn calculate_thickness(pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    var occ = 0.;
    var sca = 1.0;
    for (var i = 0; i < 9; i += 1) {
        let h = 0.01 + 0.12 * f32(i)/8.0;
        let d = -sdf(pos + h*-normal);
        occ += (h-d) * sca;
        sca *= 0.95;
        if (occ > 0.35) { break; }
    }
    return clamp(1.0 - 3.0*occ, 0.0, 1.0);
}

fn ray_intersects_aabb(ray_pos: vec3<f32>, ray_dir: vec3<f32>, bb_min: vec3<f32>, bb_max: vec3<f32>) -> bool {
    let dirfrac = 1. / ray_dir;
    let t135 = (bb_min - ray_pos) * dirfrac;
    let t246 = (bb_max - ray_pos) * dirfrac;

    let tmin = max(max(min(t135.x, t246.x), min(t135.y, t246.y)), min(t135.z, t246.z));
    let tmax = min(min(max(t135.x, t246.x), max(t135.y, t246.y)), max(t135.z, t246.z));

    // AABB behind ray
    if (tmax < 0.) {
        return false;
    }

    // ray doesn't intersect
    if (tmin > tmax) {
        return false;
    }

    return true;
}

fn aabb_intersects_aabb(a_min: vec3<f32>, a_max: vec3<f32>, b_min: vec3<f32>, b_max: vec3<f32>) -> bool {
    return (a_min.x <= b_max.x && a_max.x >= b_min.x) &&
           (a_min.y <= b_max.y && a_max.y >= b_min.y) &&
           (a_min.z <= b_max.z && a_max.z >= b_min.z);
}

fn bvh_lookup_ray(ray_pos: vec3<f32>, ray_dir: vec3<f32>) {
    var queue: array<u32, 128>;
    var sp = 0;

    // reset hit_entities
    hit_entities.count = 0u;

    // load root to queue
    queue[0] = 0u;

    loop {
        if (sp < 0) { break; }
        // pop node from stack
        let node_id = queue[sp];
        sp--;
        let node = bvh.tree[node_id];

        let ray_hit = ray_intersects_aabb(ray_pos, ray_dir, node.min, node.max);
        if (ray_hit) {
            if (node.left == -1) {
                // leaf node, right is entity data index
                hit_entities.entities[hit_entities.count] = blob_data.blobs[node.right];
                hit_entities.count++;
            } else {
                // branch node, left and right are indices for the child nodes
                // push the child nodes to queue
                queue[sp + 1] = u32(node.left);
                queue[sp + 2] = u32(node.right);
                sp += 2;
            }
        }
    }
}