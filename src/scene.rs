use bytemuck::Zeroable;
use glam::{Vec3, UVec3};

pub const SCENE_SIZE: usize = 8; // scene is 8x8x8 chunks
pub const CHUNK_SIZE: usize = 8; // chunks are 8x8x8 voxels
const NUM_MATERIALS: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TempScene {
    pub voxels: [u32;4*4*4],
}
impl TempScene {
    pub fn into_buffer(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}


#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Scene {
    size: usize, // number of chunks per dimension in this scene. Should be SCENE_SIZE.
    chunks: [Chunk;SCENE_SIZE*SCENE_SIZE*SCENE_SIZE],
    materials: [Material;NUM_MATERIALS],
}

impl Scene {
    pub fn into_buffer(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

impl Default for Scene { // change this so it actually means something
    fn default() -> Self {
        let chunks = [
            Chunk {
                size: CHUNK_SIZE,
                voxels: [Voxel::zeroed();CHUNK_SIZE*CHUNK_SIZE*CHUNK_SIZE],
            };
            SCENE_SIZE*SCENE_SIZE*SCENE_SIZE
        ];
        
        let materials = [Material::zeroed();NUM_MATERIALS];
        Self {
            size: SCENE_SIZE,
            chunks,
            materials,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Chunk {
    size: usize, // number of voxels per dimension in this chunk. Should be CHUNK_SIZE for all chunks.
    voxels: [Voxel;CHUNK_SIZE*CHUNK_SIZE*CHUNK_SIZE],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Voxel {
    material: u32, // index into material array
    normal: Vec3, // normal of this voxel
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