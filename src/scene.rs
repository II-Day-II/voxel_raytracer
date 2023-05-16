use bytemuck::Zeroable;
use glam::{Vec3, UVec3, ivec3, uvec3, vec3};

pub const SCENE_SIZE: usize = 8; // scene is 8x8x8 chunks
pub const CHUNK_SIZE: usize = 8; // chunks are 8x8x8 voxels
const NUM_MATERIALS: usize = 0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TempScene {
    pub voxels: [u32;4*4*4],
    pub size: [f32;3],
    pub padding: u32,
}
impl TempScene {
    pub fn into_buffer(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}


#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Scene {
    size: [f32;3], // number of chunks per dimension in this scene. Should be SCENE_SIZE.
    chunks: [Chunk;SCENE_SIZE*SCENE_SIZE*SCENE_SIZE],
    materials: [Material;NUM_MATERIALS],
}

impl Scene {
    pub fn into_buffer(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn new() -> Self {
        Self {
            size: [SCENE_SIZE as f32;3],
            chunks: std::array::from_fn(|i| Chunk::empty(i)),
            materials: [],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Chunk {
    pos: [f32;3], // position of this chunk in scene space
    voxels: [Voxel;CHUNK_SIZE*CHUNK_SIZE*CHUNK_SIZE],
}
impl Chunk {
    pub fn empty(i: usize) -> Self {
        Self {
            pos: expand_index(i, UVec3::ONE * SCENE_SIZE as u32),
            voxels: [Voxel::zeroed();CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
        }
    }
    pub fn fill_borders(&mut self, material: u32, albedo: UVec3) {
        let sx = self.pos[0] as u32;
        let sy = self.pos[1] as u32;
        let sz = self.pos[2] as u32;
        for z in 0..sz {
            for y in 0..sy {
                for x in 0..sx {
                    if (x == 0 || x == CHUNK_SIZE as u32) && (y == 0 || y == CHUNK_SIZE as u32) && (z == 0 || z == CHUNK_SIZE as u32) {
                        let idx = flatten_index(uvec3(x, y, z), UVec3::ONE * CHUNK_SIZE as u32);
                        let normal = vec3((x as f32 - 0.5).signum(), (y as f32 -0.5).signum(), (z as f32 - 0.5).signum());
                        self.voxels[idx] = Voxel {
                            material,
                            albedo,
                            normal,
                        };
                    }
                }
            }
        }
    }
}


#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Voxel {
    material: u32, // index into material array
    normal: Vec3, // normal of this voxel
    albedo: UVec3, // albedo of this voxel
}

impl Voxel {
    pub fn compress(&self) -> CompressedVoxel {
        let normal = ((self.normal * 255.0).as_uvec3() + 255) / 2;
        CompressedVoxel {
            normal: self.material << 24 | normal.x << 16 | normal.y << 8 | normal.z, 
            albedo: self.albedo.x << 24 | self.albedo.y << 16 | self.albedo.z << 8,
        }
    }
}

pub struct CompressedVoxel {
    normal: u32, // material index (8 bits), normal.x, normal.y, normal.z (24 bits)
    albedo: u32, // albedo.r (8), albedo.g (8), abedo.b (8), unused (8)
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


pub const fn flatten_index(position: UVec3, dimensions: UVec3) -> usize {
    (position.x + dimensions.x * (position.y + position.z * dimensions.y)) as usize
}

pub const fn expand_index(idx: usize, dimensions: UVec3) -> [f32;3] {
    let x = idx % dimensions.x as usize;
    let y = idx / dimensions.x as usize % dimensions.y as usize;
    let z = idx / (dimensions.x as usize * dimensions.y as usize);
    [x as f32, y as f32, z as f32]
}