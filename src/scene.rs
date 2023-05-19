use glam::{Vec3, UVec3, ivec3, uvec3, vec3, Vec4, Vec4Swizzles};

pub const SCENE_SIZE: usize = 8; // scene is 8x8x8 chunks
pub const CHUNK_SIZE: usize = 8; // chunks are 8x8x8 voxels
const NUM_MATERIALS: usize = 4;
const MATERIAL_EMPTY: u32 = 255;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TempScene {
    pub voxels: [u32;4*4*4],
    pub size: [f32;3],
    padding: u32,
}
#[allow(dead_code)]
impl TempScene {
    pub fn into_buffer(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}
impl Default for TempScene {
    fn default() -> Self {
        Self {
            voxels: [
                // x 0 => 4 
                // z = 0
                0,0,0,0, // y = 0
                0,1,1,0, // y = 1
                0,0,0,0, // y = 2
                0,0,0,0, // y = 3
                // z = 1
                0,1,1,0, // y = 0
                1,0,0,1, // y = 1
                1,0,0,1, // y = 2
                0,1,1,0, // y = 3
                // z = 2
                1,0,0,1, // y = 0
                0,0,0,0, // y = 1
                0,0,0,0, // y = 2
                1,0,0,1, // y = 3
                // z = 3
                1,1,1,1, // y = 0
                0,1,1,1, // y = 1
                0,0,1,1, // y = 2
                0,0,0,1, // y = 3
                ],
                size: [4.0,4.0,4.0,],
                padding: 0,    
            }
        }
}


pub struct Scene {
    size: Vec4, // number of chunks per dimension in this scene. Should be SCENE_SIZE.
    sun_direction: Vec4,
    sun_strength: Vec4,
    ambient_light: Vec4,
    time: i32,
    chunks: Vec<Chunk>,
    materials: [Material;NUM_MATERIALS],
}

impl Scene {
    pub fn into_buffer(&self) -> Vec<u8> {
        // bytemuck::bytes_of(self)
        let size = bytemuck::bytes_of(&self.size);
        let sun_pos = bytemuck::bytes_of(&self.sun_direction);
        let sun_str = bytemuck::bytes_of(&self.sun_strength);
        let ambient_str = bytemuck::bytes_of(&self.ambient_light);
        let time = bytemuck::bytes_of(&self.time);
        let chunks = bytemuck::cast_slice(&self.chunks);
        let materials = bytemuck::cast_slice(&self.materials);
        let padding = bytemuck::bytes_of(&[0u32,0,0]);
        [size, sun_pos, sun_str, ambient_str, time, padding, chunks, materials].concat() // woo, padding :)))
    }
    pub fn new() -> Self {
        let mut materials = std::array::from_fn(|_| Material::default());
        materials[0] = Material {
            emissive: false as u32,
            opacity: 1.0,
            specular: 0.0,
            ..Default::default()
        };
        materials[1] = Material {
            emissive: false as u32,
            specular: 0.8,
            opacity: 1.0,
            shininess: 3.0,
            ..Default::default()
        };
        materials[2] = Material {
            emissive: false as u32,
            specular: 0.0,
            opacity: 0.5,
            refraction_index: 1.52,
            ..Default::default()
        };
        materials[3] = Material {
            emissive: true as u32,
            specular: 0.0,
            opacity: 1.0,
            ..Default::default()
        };
        let chunks =  (0..SCENE_SIZE*SCENE_SIZE*SCENE_SIZE).map(|i| Chunk::empty(i)).collect::<Vec<_>>();
        Self {
            size: Vec4::from_array([SCENE_SIZE as f32;4]),
            sun_direction: Vec4::new(-0.5, 1.0, -0.5, 0.0),
            sun_strength: Vec4::new(0.6, 0.6, 0.6, 0.0),
            ambient_light: Vec4::new(0.01, 0.01, 0.01, 0.0),
            time: 0,
            chunks,
            materials
        }
    }
    pub fn chunk_at(&mut self, pos: UVec3) -> &mut Chunk {
        let idx = flatten_index(pos, self.size.xyz().as_uvec3());
        &mut self.chunks[idx]
    }
    pub fn update(&mut self, dt: instant::Duration) {
        self.time += dt.as_millis() as i32;
    }
    pub fn time(&self) -> i32 {
        self.time
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Chunk {
    pos: Vec4, // position of this chunk in scene space and whether or not it has visible voxels (w component)
    voxels: [CompressedVoxel;CHUNK_SIZE*CHUNK_SIZE*CHUNK_SIZE],
}
impl Chunk {
    pub fn empty(i: usize) -> Self {
        Self {
            pos: expand_index(i, UVec3::ONE * SCENE_SIZE as u32).extend(0.0),
            voxels: [Voxel::default().compress();CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
        }
    }
    pub fn fill_borders(&mut self, material: u32, albedo: UVec3) {
        for z in 0..CHUNK_SIZE as u32 {
            for y in 0..CHUNK_SIZE as u32 {
                for x in 0..CHUNK_SIZE as u32 {
                    let pos = uvec3(x, y, z);
                    if (x == 0 || x == CHUNK_SIZE as u32 - 1) || (y == 0 || y == CHUNK_SIZE as u32 - 1) || (z == 0 || z == CHUNK_SIZE as u32 - 1) {
                        let normal = (pos.as_vec3() - Vec3::ONE * CHUNK_SIZE as f32 / 2.0).normalize();
                        self.modify_voxel_at(pos, |vox| {
                            vox.material = material;
                            vox.albedo = albedo;
                            vox.normal = normal;
                        });
                    }
                }
            }
        }
    }
    pub fn fill_sphere(&mut self, material: u32, albedo: UVec3) {
        let center = Vec3::ONE * CHUNK_SIZE as f32 / 2.0;
        for z in 0..CHUNK_SIZE as u32 {
            for y in 0..CHUNK_SIZE as u32 {
                for x in 0..CHUNK_SIZE  as u32 {
                    let pos = uvec3(x, y, z).as_vec3();
                    if center.distance(pos) < CHUNK_SIZE as f32 / 2.0 {
                        let normal = (pos - center).normalize();
                        self.modify_voxel_at(pos.as_uvec3(), |vox| {
                            vox.material = material;
                            vox.albedo = albedo;
                            vox.normal = normal;
                        });
                    }
                }
            }
        }
    }
    pub fn modify_voxel_at<F>(&mut self, pos: UVec3, mut modifier: F) where F: FnMut(&mut Voxel) {
        let idx = flatten_index(pos, UVec3::ONE * CHUNK_SIZE as u32);
        let mut vox = self.voxels[idx].decompress();
        modifier(&mut vox);
        self.voxels[idx] = vox.compress();
        self.update_visibility();
    }
    fn update_visibility(&mut self) {
        self.pos.w = if self.voxels.iter().all(|v| v.normal >> 24 & 0xFF == MATERIAL_EMPTY) {0.0} else {1.0}; // make invisible if all voxels have empty material
    }
}



pub struct Voxel {
    normal: Vec3, // normal of this voxel
    albedo: UVec3, // albedo of this voxel
    material: u32, // index into material array
}

impl Voxel {
    pub fn compress(&self) -> CompressedVoxel {
        let normal = ((self.normal * 255.0) + 255.0).as_uvec3() / 2;
        CompressedVoxel {
            normal: ((self.material & 0xFF) << 24) | ((normal.x & 0xFF) << 16) | ((normal.y & 0xFF) << 8) | (normal.z & 0xFF), 
            albedo: ((self.albedo.x & 0xFF) << 24) | ((self.albedo.y & 0xFF) << 16) | ((self.albedo.z & 0xFF) << 8),
            spec_light: 0, // these fields are only used on the GPU, don't matter here
            diff_light: 0,
        }
    }
}
impl Default for Voxel {
    fn default() -> Self {
        Self {
            material: MATERIAL_EMPTY, // 255, a material that doesn't exist
            normal: Vec3::ZERO, // doesn't matter for an invisible voxel anyway
            albedo: UVec3::ONE * 255, // white
        }
    }
}


#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompressedVoxel {
    normal: u32, // material index (8 bits), normal.x, normal.y, normal.z (24 bits)
    albedo: u32, // albedo.r (8), albedo.g (8), albedo.b (8), spec.x (8)
    spec_light: u32, // spec.y(8), spec.z(8), diff.x(16)
    diff_light: u32, // diff.y(16), diff.z(16)
}

impl CompressedVoxel {
    pub fn decompress(&self) -> Voxel {
        let normal = (ivec3(
            ((self.normal >> 16) & 0xFF) as i32,
            ((self.normal >> 8) & 0xFF) as i32,
            ((self.normal >> 0) & 0xFF) as i32
        ) * 2 - 255).as_vec3() * 1.0/255.0;
        let material = self.normal >> 24;
        let albedo = uvec3(self.albedo >> 24, self.albedo >> 16, self.albedo >> 8);
        Voxel {
            material,
            normal,
            albedo,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Material {
    emissive: u32, // may need to change for padding (bool is not zeroable???)
    opacity: f32,
    refraction_index: f32,
    specular: f32,
    shininess: f32,
    // reflect type?
}
impl Default for Material {
    fn default() -> Self {
        Self {
            emissive: 0,
            opacity: 1.0,
            refraction_index: 0.0,
            specular: 0.0,
            shininess: 0.0,
        }
    }
}


pub const fn flatten_index(position: UVec3, dimensions: UVec3) -> usize {
    (position.x + dimensions.x * (position.y + position.z * dimensions.y)) as usize
}

pub const fn expand_index(idx: usize, dimensions: UVec3) -> Vec3 {
    let idx = idx as u32;
    let x = idx % dimensions.x as u32;
    let y = idx / dimensions.x as u32 % dimensions.y as u32;
    let z = idx / (dimensions.x as u32 * dimensions.y as u32);
    vec3(x as f32, y as f32, z as f32)
}