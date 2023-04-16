@group(0) @binding(0)
var screen: texture_2d<f32>;
@group(0) @binding(1)
var screen_sampler: sampler;

struct VertexToFragment {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}


@vertex
fn vs_main(@builtin(vertex_index) vert_index: u32) -> VertexToFragment {
    // vertex positions in screen space (xy) and corresponding texture coordinates (zw)
    // texture coordinates are flipped so they correspond to OpenGL
    var QUAD_POSITIONS = array<vec4<f32>, 6>(
        vec4( 1.0,  1.0, 1.0, 1.0), // top right    (1.0, 0.0)
        vec4(-1.0, -1.0, 0.0, 0.0), // bottom left  (0.0, 1.0)
        vec4( 1.0, -1.0, 1.0, 0.0), // bottom right (1.0, 1.0)

        vec4(-1.0, -1.0, 0.0, 0.0), // bottom left  (0.0, 1.0)
        vec4( 1.0,  1.0, 1.0, 1.0), // top right    (1.0, 0.0)
        vec4(-1.0,  1.0, 0.0, 1.0), // top left     (0.0, 0.0)
    ); 
    let vert = QUAD_POSITIONS[vert_index];
    var out: VertexToFragment;
    out.position = vec4<f32>(vert.xy, 0.0, 1.0);
    out.tex_coord = vert.zw;
    return out;
}

@fragment
fn fs_main(in: VertexToFragment) -> @location(0) vec4<f32> {
    return textureSample(screen, screen_sampler, in.tex_coord);
}

