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

struct Voxel {
    material: u32,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    diffuse: vec3<f32>,
    specular: vec3<f32>,
}
struct CompressedVoxel {
    normal: u32, // material index(8), x(8), y(8), z(8)
    albedo: u32, // r(8), g(8), b(8), unused(8)
}
fn decompress_voxel(in: CompressedVoxel) -> Voxel {
    var out: Voxel;
    let nr = decompress_uvec4(in.normal);
    let ar = decompress_uvec4(in.albedo);
    out.normal = vec3<f32>(vec3<i32>(nr.yzw * 2u) - 255) / 255.0; // [0..255] -> [-1..1]
    out.material = nr.x;
    out.albedo = vec3<f32>(ar.xyz) / 255.0; // [0..255] -> [0..1]
    //TODO: send lighting data from cpu
    out.diffuse = vec3(1.0);
    out.specular = vec3(0.0);
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
    chunk_map: array<Chunk, 512>, // change this!!!
    materials: array<Material, 4>, // how make dynamically sized?
}
@group(2) @binding(0)
var<storage, read> scene: Scene;

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

fn step_scene(ray: Ray, max_depth: f32, ignore_first: bool) -> StepResult {
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
                // result.new_ray.position = vec3<f32>(dda.pos); // useful for drawing chunk position
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
                result.debug = compressed.albedo & 0xFFu;//vec2(get_chunk_index(dda.pos), chunk_id);// chunk_pos / 8.0 * vec3<f32>(dda.pos) / 8.0;
                return result;
            }
            else if last_vox_id != vox_id {
                result.color_add += result.color_mul * material.opacity * vox.albedo; // * sun_strength;
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
    // return abs(info.normal); // face normal
    // return abs(vox.normal); // per-voxel normal
    // return vec3<f32>(info.new_ray.position) / f32(CHUNK_SIZE); // voxel position in chunk
    // return vox.albedo; // albedo
}

@compute @workgroup_size(16, 16, 1) // To be changed?
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

    var max_depth = -1.0; // maybe not needed?

    let scene_intersection = intersect_box(ray, vec3(0.0), scene.size.xyz);
    if scene_intersection.x > scene_intersection.y || scene_intersection.y < 0.0 { // missed map if near > far or far < 0
        out_color = skybox_color(ray.direction); 
    } else {
        //out_color = box_normal(ray_at(ray, scene_intersection.x), vec3(0.0), map_size); 
        if scene_intersection.x > 0.0 { // move the ray to the edge of the map so it can DDA inside it
            ray.position += ray.direction * (scene_intersection.x + EPSILON);
        }
        var final_normal = box_normal(ray.position, vec3(0.0), scene.size.xyz);
        
        let final_info = step_scene(ray, max_depth, false);
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
