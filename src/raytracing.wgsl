@group(0) @binding(0)
var screen: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(1)
var skybox_t: texture_cube<f32>;

@group(0) @binding(2)
var skybox_s: sampler; 

struct Camera {
    position: vec4<f32>,
    inv_view: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: Camera;

fn decompress_uvec4(in: u32) -> vec4<u32> {
    let x = (in >> 24u) & 0xFFu;
    let y = (in >> 16u) & 0xFFu;
    let z = (in >> 8u ) & 0xFFu;
    let w = (in >> 0u ) & 0xFFu;
    return vec4(x, y, z, w);
}

fn compress_uvec4(in: vec4<u32>) -> u32 {
    let in = in & vec4(0xFFu);
    return (in.x << 24u) | (in.y << 16u) | (in.z << 8u) | in.w;
}

struct Voxel {
    material: u32,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    diffuse: vec3<f32>,
    specular: vec3<f32>,
}
struct CompressedVoxel {
    normal: u32, // material index(8), x(8), y(8), z(8)
    albedo: u32, // r(8), g(8), b(8), spec.x(8)
    spec_light: u32, // spec.y(8), spec.z(8), diff.x(16)
    diff_light: u32, // diff.y(16), diff.z(16)
}
fn decompress_voxel(in: CompressedVoxel) -> Voxel {
    var out: Voxel;
    let nr = decompress_uvec4(in.normal);
    let ar = decompress_uvec4(in.albedo);
    let sr = decompress_uvec4(in.spec_light);
    let dr = decompress_uvec4(in.diff_light);

    out.normal = vec3<f32>(vec3<i32>(nr.yzw * 2u) - 255) / 255.0; // [0..255] -> [-1..1]
    out.material = nr.x;
    out.albedo = vec3<f32>(ar.xyz) / 255.0; // [0..255] -> [0..1]
    //TODO: send lighting data from cpu
    let diffuse_high = vec3(sr.z, dr.x, dr.z) << vec3(8u);
    let diffuse_low = vec3(sr.w, dr.y, dr.w);
    let diffuse = diffuse_high | diffuse_low;

    out.diffuse = vec3<f32>(diffuse) / 65535.0;
    out.specular = vec3(f32(ar.w), vec2<f32>(sr.xy)) / 255.0;
    return out;
}
struct Chunk {
    pos: vec4<f32>, // the chunk's position in the scene (x, y, z) and if chunk contains data (w = 0.0 if chunk is empty)
    voxels: array<CompressedVoxel, 512>,// don't want to hardcode the size like this ;_;
}
struct Material {
    emissive: u32,
    opacity: f32,
    refraction_index: f32,
    specular: f32,
    shininess: f32,
}
struct Scene {
    size: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_strength: vec4<f32>,
    ambient_light: vec4<f32>,
    time: u32,
    chunk_map: array<Chunk, 512>, // change this!!!
    materials: array<Material, 4>, // how make dynamically sized?
}
@group(2) @binding(0)
var<storage, read_write> scene: Scene;

// whether or not a position is within the scene
fn in_scene_bounds(pos: vec3<i32>) -> bool {
    let fpos = vec3<f32>(pos);
    return fpos.x < scene.size.x && fpos.y < scene.size.y && fpos.z < scene.size.z && fpos.x >= 0.0 && fpos.y >= 0.0 && fpos.z >= 0.0;
}

fn in_chunk_bounds(pos: vec3<i32>) -> bool {
    return pos.x < CHUNK_SIZE && pos.y < CHUNK_SIZE && pos.z < CHUNK_SIZE && pos.x >= 0 && pos.y >= 0 && pos.z >= 0;
}
fn compressed_voxel_at(chunk_id: i32, pos_in_chunk: vec3<i32>) -> CompressedVoxel {
    let idx = get_chunk_index(pos_in_chunk);
    let chunk = &scene.chunk_map[chunk_id]; // have to take a reference to index array with non-const
    return (*chunk).voxels[idx]; 
}

// the index into the scene array that corresponds to a 3d position
fn get_scene_index(pos: vec3<i32>) -> i32 {
    let isize = vec3<i32>(scene.size.xyz);
    return pos.x + isize.x * (pos.y + isize.y * pos.z);
}
fn get_chunk_index(pos: vec3<i32>) -> i32 {
    return pos.x + CHUNK_SIZE * (pos.y + CHUNK_SIZE * pos.z);
}

struct Ray {
    direction: vec3<f32>,
    inv_direction: vec3<f32>,
    position: vec3<f32>,
}
fn ray_at(ray: Ray, t: f32) -> vec3<f32> {
    return ray.position + ray.direction * t;
}

struct DDA {
    ray: Ray,
    pos: vec3<i32>, // the position in the chunk / the position of the chunk in the scene
    delta_dist: vec3<f32>, // distance ray has to travel to reach next cell in each direction
    step_dir: vec3<i32>, // direction the ray will step
    side_dist: vec3<f32>, // total distance ray has to travel to reach one additional step in each direction
}
// initialize a DDA cursor that can be stepped through the world
fn init_DDA(ray: Ray) -> DDA {
    var dda_initial: DDA;
    dda_initial.ray = ray;
    dda_initial.pos = vec3<i32>(floor(ray.position));
    dda_initial.delta_dist = abs(ray.inv_direction);
    dda_initial.step_dir = vec3<i32>(sign(ray.direction));
    dda_initial.side_dist = (sign(ray.direction) * (vec3<f32>(dda_initial.pos) - ray.position) + (sign(ray.direction) * 0.5) + 0.5) * dda_initial.delta_dist;
    return dda_initial;
}
// steps the DDA state one unit along the initial ray direction. Returns the normal of the voxel that was "hit"
fn step_DDA(state: ptr<function, DDA>) -> vec3<f32> {
    // branchless DDA from https://www.researchgate.net/publication/233899848_Efficient_implementation_of_the_3D-DDA_ray_traversal_algorithm_on_GPU_and_its_application_in_radiation_dose_calculation
    // and https://www.shadertoy.com/view/4dX3zl
    let mask: vec3<bool> = (*state).side_dist.xyz <= min((*state).side_dist.yzx, (*state).side_dist.zxy);
    (*state).side_dist += vec3<f32>(mask) * (*state).delta_dist;
    (*state).pos += vec3<i32>(mask) * (*state).step_dir;
    let normal = vec3<f32>(mask) * -vec3<f32>((*state).step_dir); 
    return normal;
}

struct StepResult {
    hit: bool, // if this is false, the rest of the data is invalid
    new_ray: Ray, // updated information about a ray after stepping 
    normal: vec3<f32>, // the face normal of the hit voxel
    voxel: Voxel,
    color_add: vec3<f32>,
    color_mul: f32,
    // TODO: refraction
    debug: u32, // to be removed, useful for drawing debug info
}

fn step_scene(ray: Ray, ignore_first: bool) -> StepResult {
    var result: StepResult;
    result.hit = false;
    result.color_add = vec3(0.0);
    result.color_mul = 1.0;
    var last_side_dist = vec3(0.0);
    var dda: DDA = init_DDA(ray);
    var normal = box_normal(ray.position, vec3(0.0), scene.size.xyz);
    while in_scene_bounds(dda.pos) {
        let chunk_id = get_scene_index(dda.pos);
        let chunk = scene.chunk_map[chunk_id];
        if chunk.pos.w != 0.0 {  // the chunk has non-empty voxels
            var chunk_ray: Ray = dda.ray; // ray to use for traversing in the chunk
            let updated_ray_pos = dda.ray.position + dda.ray.direction * (min(min(last_side_dist.x, last_side_dist.y), last_side_dist.z) - EPSILON); // move to the chunk bounds
            chunk_ray.position = clamp((updated_ray_pos - vec3<f32>(dda.pos)) * vec3(f32(CHUNK_SIZE)), vec3(EPSILON), vec3(f32(CHUNK_SIZE)) - EPSILON); // set position relative to chunk bounds
            result = step_chunk(chunk_ray, chunk_id, result);
            if result.hit {
                result.new_ray.position = vec3<f32>(dda.pos) + result.new_ray.position / f32(CHUNK_SIZE); // hit position in scene space
                return result;
            }
        }
        last_side_dist = dda.side_dist;
        normal = step_DDA(&dda);
    }
    return result;
}

var<private> last_vox_id: u32 = 255u; // the last hit voxel's albedo and material, used for transparency
var<private> last_vox_refract: f32 = 1.0; // last hit voxel's refraction index, used for TODO: refraction

fn step_chunk(chunk_ray: Ray, chunk_id: i32, partial_result: StepResult) -> StepResult {
    var result: StepResult = partial_result;
    result.hit = false;
    var last_side_dist = vec3(0.0);
    var dda: DDA = init_DDA(chunk_ray);
    var normal = box_normal(chunk_ray.position, vec3(0.0), vec3(f32(CHUNK_SIZE)));
    while in_chunk_bounds(dda.pos) {
        let compressed = compressed_voxel_at(chunk_id, dda.pos);
        let vox_id = (compressed.albedo & 0xFFFFFF00u) | (compressed.normal >> 24u);
        let vox = decompress_voxel(compressed); 
        if vox.material != 255u { // would be a constant for MATERIAL_EMPTY instead of 255
            let material = scene.materials[vox.material];
            if material.opacity >= 1.0 {
                result.hit = true;
                result.new_ray.position = vec3<f32>(dda.pos) + dda.ray.direction * (min(min(last_side_dist.x, last_side_dist.y), last_side_dist.z) - EPSILON);
                result.normal = normal;
                result.voxel = vox;
                result.debug = compressed.albedo & 0xFFu;//vec2(get_chunk_index(dda.pos), chunk_id);
                return result;
            }
            else if last_vox_id != vox_id {
                result.color_add += result.color_mul * material.opacity * vox.albedo * scene.sun_strength.xyz;
                result.color_mul *= 1.0 - material.opacity;
                //TODO: refraction
                last_vox_id = vox_id;
                last_vox_refract = material.refraction_index;
            }
        }
        else if last_vox_id != 255u {
            // TODO: refraction
            last_vox_id = 255u;
            last_vox_refract = 1.0;
        }
        last_side_dist = dda.side_dist;
        normal = step_DDA(&dda);
    }
    return result;
}

var<private> EPSILON: f32 = 0.0001; // I have to do this instead of constants at the moment, since Naga doesn't have constants yet.
//var<private> CHUNK_SIZE: vec3<i32> = vec3(8); // THIS CONST EXPR ISN'T IMPLEMENTED
var<private> CHUNK_SIZE: i32 = 8;


fn mandelbrot(pos: vec2<f32>) -> vec3<f32> {
    let MAX_ITERS: i32 = 100;
    var z: vec2<f32> = vec2(0.0);
    let c = pos.xy * 2.0;
    let epsilon = 1000.0;
    for (var i: i32 = 0; i < MAX_ITERS; i++) {
        if length(z) > epsilon {
            return (f32(i) / f32(MAX_ITERS)) + vec3(0.15, 0.25, 0.8); 
        }
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
    }
    return vec3(0.0);
}

fn intersect_box(ray: Ray, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let tMin = (box_min - ray.position) * ray.inv_direction;
    let tMax = (box_max - ray.position) * ray.inv_direction;

    let t1 = min(tMin, tMax);
    let t2 = max(tMin, tMax);

    let near = max(max(t1.x, t1.y), t1.z);
    let far = min(min(t2.x, t2.y), t2.z);

    return vec2(near, far);
}
fn box_normal(intersect_pos: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec3<f32> {
    let c = (box_min + box_max) * 0.5;
    let p = intersect_pos - c;
    let d = (box_max - box_min) * 0.5;
    let bias: f32 = 1.0 + EPSILON;
    return normalize(trunc(p / d * bias));
}

fn skybox_color(direction: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(skybox_t, skybox_s, direction, 0.0).xyz;
}
fn voxel_color(info: StepResult) -> vec3<f32> {
    let vox = info.voxel;
    let material = scene.materials[vox.material];
    var solid_color: vec3<f32>;
    if material.emissive != 0u { // material is emissive
        solid_color = vox.albedo;
    } else {
        solid_color = vox.albedo * vox.diffuse + vox.specular;
    }
    return solid_color * info.color_mul + info.color_add; // total lighting 
    // return solid_color * sin(f32(scene.time / 10)) * info.color_mul + info.color_add; // visualize time
    // return abs(info.normal); // face normal
    // return abs(vox.normal); // per-voxel normal
    // return vec3<f32>(info.new_ray.position) / f32(CHUNK_SIZE); // voxel position in chunk
    // return vox.albedo; // albedo
}

@compute @workgroup_size(16, 16, 1) // Does the raytracing from the camera to the closest voxel, drawing the color to the final texture.
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    var out_color: vec3<f32>;    

    let texture_pos = vec2<i32>(global_id.xy); // cast to i32 so we can use in textureStore
    let texture_dim = textureDimensions(screen);
    let screen_pos = (vec2<f32>(texture_pos) / vec2<f32>(texture_dim)) * 2.0 - 1.0; // pixel position in screen space
    
    var ray_pos: vec3<f32> = camera.position.xyz;
    var inv_view_centered: mat4x4<f32> = camera.inv_view; // the camera's inverse view matrix but without the translation
    inv_view_centered[3] = vec4(0.0, 0.0, 0.0, 1.0);
    let ray_dir = normalize((inv_view_centered * camera.inv_proj * vec4(screen_pos, 0.0, 1.0)).xyz) + EPSILON;

    var ray: Ray;
    ray.position = ray_pos;
    ray.direction = ray_dir;
    ray.inv_direction = 1.0 / ray_dir;

    let scene_intersection = intersect_box(ray, vec3(0.0), scene.size.xyz);
    if scene_intersection.x > scene_intersection.y || scene_intersection.y < 0.0 { // missed map if near > far or far < 0
        out_color = skybox_color(ray.direction); 
    } else {
        //out_color = box_normal(ray_at(ray, scene_intersection.x), vec3(0.0), map_size); 
        if scene_intersection.x > 0.0 { // move the ray to the edge of the map so it can DDA inside it
            ray.position += ray.direction * (scene_intersection.x + EPSILON);
        }
        var final_normal = box_normal(ray.position, vec3(0.0), scene.size.xyz);
        
        let final_info = step_scene(ray, false);
        if final_info.hit {
            //let dist = distance(ray_pos, final_info.new_ray.position) / (scene.size.x * 4.0);
            //out_color = vec3(1.0-dist);
            out_color = voxel_color(final_info);
        } else {
            out_color = skybox_color(ray.direction) * final_info.color_mul + final_info.color_add;
        }
    }
    //out_color = mandelbrot((screen_pos - vec2(0.25, 0.0)) * vec2(1.0, 0.75));
    //out_color = vec3(normalize(scene_intersection.xy), 0.0);
    //out_color = vec3(vec2<f32>(texture_pos)/vec2<f32>(texture_dim), 0.0);
    //out_color = vec3(screen_pos, 0.0);
    textureStore(screen, texture_pos, vec4<f32>(out_color, 1.0));
}


// Performs lighting calculations for every voxel in the scene, storing the output in the voxels themselves
@compute @workgroup_size(8, 8, 8)
fn lighting_main(@builtin(workgroup_id) wg_id: vec3<u32>, @builtin(local_invocation_id) invoc_id: vec3<u32>) {

    let num_diffuse_samples = 1;

    let scene_pos = vec3<i32>(wg_id);
    let pos_in_chunk = vec3<i32>(invoc_id);
    let scene_idx = get_scene_index(scene_pos);
    let chunk_idx = get_chunk_index(pos_in_chunk);
    if !in_chunk_bounds(pos_in_chunk) || !in_scene_bounds(scene_pos) || scene.chunk_map[chunk_idx].pos.w == 0.0 { // don't bother with lighting for empty or oob chunks
        return;
    }
    let compressed = compressed_voxel_at(scene_idx, pos_in_chunk);
    let this_voxel = decompress_voxel(compressed);
    let this_material = scene.materials[this_voxel.material];
    // start the ray at the center of the voxel
    let inv_chunk_size = vec3(1.0/f32(CHUNK_SIZE)); 
    let half_inv_chunk_size = inv_chunk_size / 2.0;
    let ray_pos: vec3<f32> = vec3<f32>(pos_in_chunk) * inv_chunk_size // the number of eights of a chunk away from the chunk origin corner
        + vec3<f32>(scene_pos) // the chunk origin corner
        + half_inv_chunk_size  // to the middle of the voxel
        + (half_inv_chunk_size - vec3(EPSILON)) * this_voxel.normal; // bias slightly on the normal of the vector
    
    // TODO: should be moved into a struct probably
    var spec_light: vec3<f32> = vec3(0.0);
    var diff_light: vec3<f32> = vec3(0.0);
    
    let view_dir = normalize(ray_pos - camera.position.xyz);
    // specular rays
    if this_material.specular > 0.0 && dot(view_dir, this_voxel.normal) < 0.0 { // view ray is looking at the voxel face from the front
        var sphere_points = array<vec3<f32>,15>( // points on unit sphere, taken from DoonEngine. Must be var since there are no consts and let-bindings can't be indexed by non consts
            vec3(0.000000, 1.000000, 0.000000), 
            vec3(-0.379803, 0.857143, 0.347931), 
            vec3(0.061185, 0.714286, -0.697174), 
            vec3(0.499316, 0.571429, 0.651270), 
            vec3(-0.889696, 0.428571, -0.157375), 
            vec3(0.808584, 0.285714, -0.514354), 
            vec3(-0.256942, 0.142857, 0.955810), 
            vec3(-0.460906, 0.000000, -0.887449), 
            vec3(0.929687, -0.142857, 0.339521), 
            vec3(-0.885815, -0.285714, 0.365650), 
            vec3(0.382949, -0.428571, -0.818338), 
            vec3(0.245607, -0.571429, 0.783037), 
            vec3(-0.605521, -0.714286, -0.350913), 
            vec3(0.503065, -0.857143, -0.110596), 
            vec3(-0.000000, -1.000000, 0.000000)
        );

        let reflected = reflect(view_dir, this_voxel.normal);
        for (var i: i32 = 0; i < 15; i++) {
            let specular_dir = normalize(reflected * this_material.shininess + sphere_points[i]) + EPSILON;
            var spec_ray: Ray;
            spec_ray.direction = specular_dir;
            spec_ray.inv_direction = 1.0 / specular_dir;
            spec_ray.position = ray_pos;
            spec_light = specular_ray(scene_pos, spec_ray, this_voxel, spec_light);
        }
        spec_light /= 15.0;
    }
    // diffuse + shadow rays
    if this_material.specular < 1.0 {
        for (var i: i32 = 0; i < num_diffuse_samples; i++) {
            diff_light += scene.ambient_light.xyz;
            diff_light = diffuse_ray(ray_pos, this_voxel, scene.time * (u32(i) + 1u), diff_light);
            diff_light = shadow_ray(ray_pos, scene.time * (u32(i) + 2u), diff_light);
        }
        diff_light = (diff_light + this_voxel.diffuse) / f32(num_diffuse_samples);
    }
    // as this will be assumed to be in the range 0-1 when compressing and decompressing:
    spec_light = clamp(spec_light, vec3(0.0), vec3(1.0));
    diff_light = clamp(diff_light, vec3(0.0), vec3(1.0));

    let store_diffuse = vec3<u32>(round(diff_light * 65535.0));
    let diffuse_low_bytes = store_diffuse & vec3(0xFFu);
    let diffuse_high_bytes = (store_diffuse >> vec3(8u)) & vec3(0xFFu);
    scene.chunk_map[scene_idx].voxels[chunk_idx].albedo = compress_uvec4(vec4(vec3<u32>(round(this_voxel.albedo * 255.0)), u32(round(spec_light.x * 255.0))));
    // scene.chunk_map[scene_idx].voxels[chunk_idx].albedo = compress_uvec4(vec4(vec3(255u,0u,0u), u32(round(spec_light.x * 255.0))));
    scene.chunk_map[scene_idx].voxels[chunk_idx].spec_light = compress_uvec4(vec4(vec2<u32>(round(spec_light.yz * 255.0)), diffuse_high_bytes.x, diffuse_low_bytes.x));
    scene.chunk_map[scene_idx].voxels[chunk_idx].diff_light = compress_uvec4(vec4(diffuse_high_bytes.y, diffuse_low_bytes.y, diffuse_high_bytes.z, diffuse_low_bytes.z));
}

// cast a specular ray from vox at scene_pos, accumulating color in spec_light, which is returned
fn specular_ray(scene_pos: vec3<i32>, ray: Ray, vox: Voxel, spec_light: vec3<f32>) -> vec3<f32> {
    let spec_bounce_limit = 2; // should maybe come from CPU
    
    var spec_light: vec3<f32> = spec_light;

    var last_pos: vec3<f32> = ray.position;
    var multiplier: vec3<f32> = vox.albedo; // accumulate color over the bounces
    var mut_ray: Ray = ray;
    for(var i: i32; i < spec_bounce_limit; i++) {
        let info = step_scene(mut_ray, true); // TODO: take ignore_first into account
        if info.hit {
            let dist = abs(floor(info.new_ray.position * f32(CHUNK_SIZE)) - floor(last_pos * f32(CHUNK_SIZE)));
            if dot(dist, dist) <= 1.0 { // ray hit the voxel next to the one being lit, meaning it's occluded from here
                return spec_light;
            }
            var hit_voxel: Voxel = info.voxel;
            let hit_material = scene.materials[hit_voxel.material];
            hit_voxel.diffuse *= 1.0 - hit_material.specular;
            if hit_material.emissive != 0u {
                return spec_light + (hit_voxel.albedo * info.color_mul + info.color_add) * multiplier * vox.albedo;
            } else {
                let hit_color = hit_voxel.diffuse * hit_voxel.albedo;
                spec_light += (hit_color * info.color_mul + info.color_add) * multiplier;
                if hit_material.specular == 0.0 { // no more reflections needed
                    return spec_light;
                }
                // bounce the ray
                multiplier *= hit_voxel.albedo * info.color_mul * hit_material.specular;
                last_pos = info.new_ray.position;
                mut_ray.position = info.new_ray.position;
                mut_ray.direction = reflect(mut_ray.direction, hit_voxel.normal);
                mut_ray.inv_direction = 1.0 / mut_ray.direction;
            }
        }
        else if dot(mut_ray.direction, scene.sun_direction.xyz) > 0.99 { // specular highlight
            return spec_light + scene.sun_strength.xyz * info.color_mul + info.color_add;
        }
        else { // reflect sky color
            return spec_light + (skybox_color(mut_ray.direction) * info.color_mul + info.color_add) * multiplier;
        }
    }
    return spec_light; // this should never be reachable
}

// cast a diffuse ray from vox at ray_pos, accumulating color in diff_light
fn diffuse_ray(ray_pos: vec3<f32>, vox: Voxel, rng: u32, diff_light: vec3<f32>) -> vec3<f32> {
    let diffuse_bounce_limit = 6; // TODO: move to a uniform
   
    var rng = rng;
   
    var new_color: vec3<f32> = vec3(1.0);
    var last_pos: vec3<f32> = ray_pos;
    var last_dir: vec3<f32>;
    var hit_normal = vox.normal;
    var hit_mat: Material;
    var mut_ray: Ray;
    var info: StepResult;
    mut_ray.position = ray_pos;
    for (var i: i32 = 0; i < diffuse_bounce_limit; i++) {
        if i > 0 { // sometimes reflect ray on later bounces
            if rand(&rng) < hit_mat.specular {
                mut_ray.direction = normalize(reflect(last_dir, hit_normal) * hit_mat.shininess + rand_unit_sphere(&rng)); 
            }
        } else {
            //TODO: randomize after first sample
            mut_ray.direction = normalize(hit_normal);
        }
        mut_ray.inv_direction = 1.0 / mut_ray.direction;
        info = step_scene(mut_ray, true);
        hit_normal = info.voxel.normal;
        hit_mat = scene.materials[info.voxel.material];
        if info.hit {
            mut_ray.position = info.new_ray.position;
            let dist = abs(floor(last_pos * f32(CHUNK_SIZE)) - floor(info.new_ray.position * f32(CHUNK_SIZE)));
            if dot(dist, dist) <= 1.0 { // hit adjacent voxel, meaning it's occluded
                return diff_light;
            } 
            if hit_mat.emissive != 0u {
                return diff_light + new_color * (info.voxel.albedo * info.color_mul + info.color_add);
            }
            else {
                new_color *= (info.voxel.albedo * info.color_mul + info.color_add);
            }
        }
        else {
            return diff_light + new_color * max(dot(mut_ray.direction, scene.sun_direction.xyz), 0.0) * scene.sun_strength.xyz * info.color_mul + info.color_add;
        }
        last_dir = mut_ray.direction;
    }
    return diff_light;
}

// cast a ray toward the sun, adding the sun's light if the ray doesn't hit the world
fn shadow_ray(ray_pos: vec3<f32>, rng: u32, diff_light: vec3<f32>) -> vec3<f32> {
    var sun_ray: Ray;
    sun_ray.position = ray_pos;
    sun_ray.direction = scene.sun_direction.xyz + EPSILON;
    // TODO: if first_sample randomize the direction a bit
    sun_ray.inv_direction = 1.0 / sun_ray.direction;    

    let info = step_scene(sun_ray, true);
    if !info.hit {
        return diff_light + scene.sun_strength.xyz * info.color_mul + info.color_add;
    }

    return diff_light;
}

// Random number functions taken from Sebastian Lague's raytracing video
// random float in [0..1]
fn rand(seed: ptr<function,u32>) -> f32 {
    return f32(next_random_number(seed)) / 4294967295.0; // 2^32 - 1
}

fn rand_unit_sphere(seed: ptr<function, u32>) -> vec3<f32> {
    let x = rand_normal_dist(seed);
    let y = rand_normal_dist(seed);
    let z = rand_normal_dist(seed);

    return normalize(vec3(x, y, z));
}

fn rand_normal_dist(seed: ptr<function, u32>) -> f32 {
    let theta = 2.0 * 3.1415926 * rand(seed);
    let rho = sqrt(-2.0 * log(rand(seed)));
    return rho * cos(theta);
}

fn next_random_number(seed: ptr<function,u32>) -> u32 {
    *seed = *seed * 747796405u + 2891336453u;
    var result: u32 = ((*seed >> ((*seed >> 28u) + 4u)) ^ *seed) * 277803737u;
    result = (result >> 22u) ^ result;
    return result;
}