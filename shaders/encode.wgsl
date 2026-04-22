// encode.wgsl — JAB code RGBA render compute shader.
//
// Dispatched as a 2-D grid: one workgroup per module row,
// one invocation per module column.  Each invocation fills
// `module_px × module_px` pixels in the output texture.
//
// Uniforms:
//   side        – symbol side in modules
//   module_px   – pixels per module side
//   img_width   – output image width in pixels
//
// Storage buffers:
//   modules[]   – u32, one per module (color index)
//   palette[]   – u32, RGBA packed (one per palette entry)
//   output[]    – u32, RGBA packed pixels (write-only)

struct Uniforms {
    side       : u32,
    module_px  : u32,
    img_width  : u32,
    _pad       : u32,
};

@group(0) @binding(0) var<uniform>             uni     : Uniforms;
@group(0) @binding(1) var<storage, read>        modules : array<u32>;
@group(0) @binding(2) var<storage, read>        palette : array<u32>;
@group(0) @binding(3) var<storage, read_write>  output  : array<u32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let mr = gid.y; // module row
    let mc = gid.x; // module col
    if (mr >= uni.side || mc >= uni.side) { return; }

    let color_idx = modules[mr * uni.side + mc];
    let rgba_packed = palette[color_idx];

    let px_base_r = mr * uni.module_px;
    let px_base_c = mc * uni.module_px;

    for (var dy: u32 = 0u; dy < uni.module_px; dy++) {
        for (var dx: u32 = 0u; dx < uni.module_px; dx++) {
            let px = (px_base_r + dy) * uni.img_width + (px_base_c + dx);
            output[px] = rgba_packed;
        }
    }
}
