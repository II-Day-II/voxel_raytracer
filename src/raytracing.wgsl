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


struct Scene {
    chunk_map: array<u32, 64>, // change this!!!
}
@group(2) @binding(0)
var<storage, read> scene: Scene;

struct Ray {
    direction: vec3<f32>,
    inv_direction: vec3<f32>,
    position: vec3<f32>,
}
fn ray_at(ray: Ray, t: f32) -> vec3<f32> {
    return ray.position + ray.direction * t;
}

var<private> EPSILON: f32 = 0.0001;

fn mandelbrot(pos: vec2<f32>) -> vec3<f32> {
    let MAX_ITERS: i32 = 100;
    var z: vec2<f32> = vec2(0.0, 0.0);
    let c = pos.xy * 2.0;
    let epsilon = 1000.0;
    for (var i: i32 = 0; i < MAX_ITERS; i++) {
        if length(z) > epsilon {
            return (f32(i) / f32(MAX_ITERS)) + vec3(0.15, 0.25, 0.8); 
        }
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
    }
    return vec3(0.0, 0.0, 0.0);
}

// stolen from DoonEngine
fn intersect_cube(ray: Ray, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let tMin = (box_min - ray.position) * ray.inv_direction;
    let tMax = (box_max - ray.position) * ray.inv_direction;

    let t1 = min(tMin, tMax);
    let t2 = max(tMin, tMax);

    let near = max(max(t1.x, t1.y), t1.z);
    let far = min(min(t2.x, t2.y), t2.z);

    return vec2(near, far);
}
fn cube_normal(intersect_pos: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec3<f32> {
    let c = (box_min + box_max) * 0.5;
    let p = intersect_pos - c;
    let d = (box_max - box_min) * 0.5;
    let bias: f32 = 1.0 + EPSILON;
    return normalize(trunc(p / d * bias));
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

    let map_size: vec3<f32> = vec3(4.0, 4.0, 4.0); // TODO: move this to the map struct when it exists
    let scene_intersection = intersect_cube(ray, vec3(0.0), map_size);
    if scene_intersection.x > scene_intersection.y || scene_intersection.y < 0.0 { // missed map if near > far or far < 0
        out_color = textureSampleLevel(skybox_t, skybox_s, ray.direction, 0.0).xyz; 
    } else {
        // TODO: step through the map when it exists
        out_color = cube_normal(ray_at(ray, scene_intersection.x), vec3(0.0), map_size); 
    }
    //out_color = mandelbrot((screen_pos - vec2(0.25, 0.0)) * vec2(1.0, 0.75));
    //out_color = vec3(normalize(scene_intersection.xy), 0.0);
    //out_color = vec3(vec2<f32>(texture_pos)/vec2<f32>(texture_dim), 0.0);
    //out_color = vec3(screen_pos, 0.0);
    textureStore(screen, texture_pos, vec4<f32>(out_color, 1.0));
}
