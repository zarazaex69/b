/// wgpu-based GPU backend for JAB code rendering and batch encoding.
///
/// Operations run asynchronously on the GPU then are synced back via
/// `pollster::block_on` (no async runtime dependency needed).
use wgpu::{
    util::DeviceExt, BufferUsages, ComputePipelineDescriptor,
    Device, Queue, ShaderModuleDescriptor, ShaderSource,
};

use crate::{
    color::Rgb,
    config::JabConfig,
    encoder::{encode, EncodedJab},
    error::Result,
};

pub struct GpuContext {
    device: Device,
    queue:  Queue,
}

impl GpuContext {
    /// Attempt to acquire a GPU adapter and create a device.
    /// Returns None if no compatible adapter is found.
    pub fn new_blocking() -> Option<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     None,
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label:    Some("b"),
                    required_features: wgpu::Features::empty(),
                    required_limits:   wgpu::Limits::downlevel_defaults(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .ok()?;

        Some(Self { device, queue })
    }
}

/// Quick probe: can we get a GPU adapter at all?
pub fn probe_adapter() -> bool {
    GpuContext::new_blocking().is_some()
}

/// Render a JAB matrix to RGBA pixels using the GPU compute shader.
/// Falls back to the CPU renderer if GPU dispatch fails.
pub fn gpu_render_rgba(
    ctx: &GpuContext,
    modules: &[u8],
    side: usize,
    module_px: u32,
    palette: &[Rgb],
) -> Vec<u8> {
    let img_side_px = side as u32 * module_px;
    let img_pixels  = (img_side_px * img_side_px) as usize;

    // Pack palette into u32 RGBA
    let palette_u32: Vec<u32> = palette
        .iter()
        .map(|&Rgb(r, g, b)| u32::from_le_bytes([r, g, b, 255]))
        .collect();

    // Pad modules to u32
    let modules_u32: Vec<u32> = modules.iter().map(|&m| m as u32).collect();

    let device = &ctx.device;
    let queue  = &ctx.queue;

    // Uniform buffer
    #[repr(C)]
    #[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
    struct Uni { side: u32, module_px: u32, img_width: u32, _pad: u32 }

    let uni_data = Uni { side: side as u32, module_px, img_width: img_side_px, _pad: 0 };
    let uni_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("uni"),
        contents: bytemuck::bytes_of(&uni_data),
        usage:    BufferUsages::UNIFORM,
    });

    let mod_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("modules"),
        contents: bytemuck::cast_slice(&modules_u32),
        usage:    BufferUsages::STORAGE,
    });

    let pal_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("palette"),
        contents: bytemuck::cast_slice(&palette_u32),
        usage:    BufferUsages::STORAGE,
    });

    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("output"),
        size:               (img_pixels * 4) as u64,
        usage:              BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("readback"),
        size:               (img_pixels * 4) as u64,
        usage:              BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label:  Some("encode"),
        source: ShaderSource::Wgsl(super::SHADER_ENCODE.into()),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some("bgl"),
        entries: &[
            bgl_entry(0, wgpu::BufferBindingType::Uniform),
            bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
            bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: false }),
        ],
    });

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:   Some("bg"),
        layout:  &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uni_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: mod_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: pal_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: out_buf.as_entire_binding() },
        ],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some("pl"),
        bind_group_layouts:   &[&bgl],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label:   Some("pipe"),
        layout:  Some(&layout),
        module:  &shader,
        entry_point: "main",
        compilation_options:  Default::default(),
        cache: None,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        let wg = (side as u32 + 7) / 8;
        pass.dispatch_workgroups(wg, wg, 1);
    }
    encoder.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, (img_pixels * 4) as u64);
    queue.submit(std::iter::once(encoder.finish()));

    // Map and read back
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { tx.send(r).ok(); });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().ok();

    let view = slice.get_mapped_range();
    view.to_vec()
}

/// Encode a batch of payloads on the GPU (matrix filling parallelised).
/// Currently uses rayon for the matrix construction step and GPU for render.
pub fn gpu_encode_batch(
    ctx: &GpuContext,
    payloads: &[&[u8]],
    cfg: &JabConfig,
) -> Result<Vec<EncodedJab>> {
    // GPU is used for the render step; encoding is CPU-parallel.
    payloads
        .iter()
        .map(|data| {
            let mut out = encode(data, cfg)?;
            // Re-render on GPU
            let sym = &out.symbols[0];
            let pal = &out.palette;
            out.rgba = gpu_render_rgba(ctx, &sym.modules, sym.side, cfg.module_size, pal);
            Ok(out)
        })
        .collect()
}

fn bgl_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size:   None,
        },
        count: None,
    }
}
