use glam::uvec3;
#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;
use wgpu::{util::DeviceExt, include_wgsl};
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, Window},
};


mod camera;
use camera::Camera;
mod texture;
mod model;
mod scene;
mod resources;


#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniform {
    position: [f32; 3],
    _padding: u32, // struct size needs to be a power of 2 for uniforms, separating individual vec3s
    color: [f32; 3],
    _padding2: u32,
}

struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Window,
    
    screen_render_pipeline: wgpu::RenderPipeline,
    screen_render_bind_group: wgpu::BindGroup,
    raytrace_compute_pipeline: wgpu::ComputePipeline,
    lighting_compute_pipeline: wgpu::ComputePipeline,
    raytrace_bind_group: wgpu::BindGroup,
    
    srgb_format: wgpu::TextureFormat,
    depth_texture: texture::Texture,
    screen_texture: texture::Texture,
    skybox: texture::Texture,
    
    is_mouse_pressed: bool,
    
    camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    
    clear_color: wgpu::Color,

    scene_bind_group: wgpu::BindGroup,
    scene_buffer: wgpu::Buffer,
    scene: scene::Scene,
}

impl State {
    // Creating some of the wgpu types requires async code
    async fn new(window: Window) -> Self {

        // SURFACE, ADAPTER, QUEUE ---- HARDWARE STUFF
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });
        
        // # Safety
        // The surface needs to live as long as the window that created it.
        // State owns the window so this should be safe.
        let surface = unsafe { instance.create_surface(&window) }.unwrap();

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).await.unwrap();
        
        let mut limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };
        limits.max_compute_invocations_per_workgroup = 512; // TODO: remove the need for this by refactoring lighting shader
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                limits,
                label: None,
            },
            None,
        ).await.unwrap();
        
        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps.formats.iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoNoVsync, // frames go brrr
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // CAMERA --------------------
        let camera = Camera::new(
            (-4.0, 4.0, -4.0).into(), // slightly away from scene
            45.0f32.to_radians(), // looking diagonally along xz
            -25.0f32.to_radians(), // looking slightly down
            config.width as f32 / config.height as f32,
            59.0f32.to_radians(), // vertical fov corresponding to 90 degrees horizontal on a 16:9 screen
            0.1,
            100.0,
        );
       
        let camera_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Camera buffer"),
                contents: bytemuck::bytes_of(&camera.uniform()),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
        let camera_bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor { 
                label: Some("camera_bind_group_layout"), 
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer { 
                            ty: wgpu::BufferBindingType::Uniform, 
                            has_dynamic_offset: false, 
                            min_binding_size: None, 
                        },
                        count: None,
                    },
                ],
            }
        );
        let camera_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                layout: &camera_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: camera_buffer.as_entire_binding(),
                    },
                ],
                label: Some("camera_bind_group"),
            }
        );
        
        // TEXTURES -----------------
        let srgb_format = wgpu::TextureFormat::Rgba8Unorm;
        let screen_texture = texture::Texture::create_screen_texture(&device, &config, srgb_format);
        let skybox = texture::Texture::create_cubemap(&device, &queue, "skybox").await;
        
        // LIGHTS -------------
        

        // WORLD -----------------
        let mut scene = scene::Scene::new();
        scene.spawn_ground_plane();
        scene.chunk_at(uvec3(0,0,0)).fill_borders(0, uvec3(180, 180, 180));
        scene.chunk_at(uvec3(0,1,0)).fill_sphere(0, uvec3(180, 180, 180));
        scene.chunk_at(uvec3(1, 1, 1)).fill_borders(2, uvec3(255, 255, 84));
        scene.chunk_at(uvec3(2, 2, 2)).fill_sphere(2, uvec3(210, 115, 80));
        scene.chunk_at(uvec3(3, 1, 3)).fill_sphere(3, uvec3(0, 190, 0));
        let scene_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("scene buffer"),
                contents: &scene.into_buffer(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, // must be storage, so we can read and write in shader
            }
        );
        let scene_bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor { 
                label: Some("scene bind group layout"), 
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }
                ],
            }
        );
        let scene_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor { 
                label: Some("scene bind group"),
                layout: &scene_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { 
                        binding: 0,
                        resource: scene_buffer.as_entire_binding(),
                    },
                ],
            }
        );

        // SHADERS AND RENDER PIPELINES ------------------------
        
        let (screen_render_bind_group, screen_render_bind_group_layout) = create_screen_bind_group(&device, &screen_texture);

        let screen_render_pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("Screen Render Pipeline Layout"),
                bind_group_layouts: &[
                    &screen_render_bind_group_layout,
                ],
                push_constant_ranges: &[],
            }
        );

        let screen_render_pipeline = {
            let shader = include_wgsl!("screen_shader.wgsl");

            create_render_pipeline(
                &device, 
                &screen_render_pipeline_layout, 
                config.format, 
                None,
                &[],
                shader,
            )
        };

        let (raytrace_bind_group, raytrace_bind_group_layout) = create_raytrace_bind_group(&device, &screen_texture, srgb_format, &skybox);

        let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Raytracing compute pipeline layout"),
            bind_group_layouts: &[
                &raytrace_bind_group_layout,
                &camera_bind_group_layout,
                &scene_bind_group_layout,
            ], // add the world and camera here
            push_constant_ranges: &[]
        });
        let raytrace_shader = include_wgsl!("raytracing.wgsl");
        let raytrace_module = device.create_shader_module(raytrace_shader);
        let raytrace_compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Raytracing compute pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &raytrace_module,
                entry_point: "main",
            }
        );
        let lighting_compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Lighting compute pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &raytrace_module,
            entry_point: "lighting_main",
        });
        

        // DEPTH BUFFER --------
        let depth_texture = texture::Texture::create_depth_texture(&device, &config, "depth_texture");
        // TODO: find out how to use this in the compute shader if necessary

        let is_mouse_pressed = false;



        Self {
            clear_color: wgpu::Color::BLACK,
            
            window,
            surface,
            device,
            queue,
            config,
            size,
            
            screen_render_pipeline,
            screen_render_bind_group,
            raytrace_compute_pipeline,
            lighting_compute_pipeline,
            raytrace_bind_group,
            
            srgb_format,
            depth_texture,
            screen_texture,
            skybox,
            
            is_mouse_pressed,
            
            camera,
            camera_buffer,
            camera_bind_group,
            
            scene_bind_group,
            scene_buffer,
            scene,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.camera.projection.resize(new_size.width, new_size.height);
            self.depth_texture = texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");
            self.screen_texture = texture::Texture::create_screen_texture(&self.device, &self.config, self.srgb_format);
            self.screen_render_bind_group = create_screen_bind_group(&self.device, &self.screen_texture).0;
            self.raytrace_bind_group = create_raytrace_bind_group(&self.device, &self.screen_texture, self.srgb_format, &self.skybox).0;
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {    
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let xt = position.x as f64 / self.size.width as f64;
                let yt = position.y as f64 / self.size.height as f64;
                let tl = glam::dvec3(1., 0., 0.);
                let tr = glam::dvec3(0., 0., 1.);
                let bl = glam::dvec3(0., 1., 0.);
                let br = glam::dvec3(1., 1., 0.);
                let t = tl.lerp(tr, xt);
                let b = bl.lerp(br, xt);
                let color = t.lerp(b, yt);
                self.clear_color = wgpu::Color {
                    r: color.x,
                    g: color.y,
                    b: color.z,
                    a: 1.0
                };
                false
            },
            WindowEvent::KeyboardInput { 
                input: KeyboardInput {
                    state, 
                    virtual_keycode: Some(key),
                    .. 
                },
                .. 
            } => {self.camera.controller.process_keyboard(*key, *state)},
            WindowEvent::MouseInput{
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.is_mouse_pressed = *state == ElementState::Pressed;
                true
            },
            WindowEvent::MouseWheel { delta, .. } => {
                self.camera.controller.process_scroll(delta);
                true
            }
            _ => false,
        }
    }
    
    fn update(&mut self, dt: instant::Duration) {
        self.camera.update(dt);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&self.camera.uniform()));
        self.scene.update(dt);
        self.queue.write_buffer(&self.scene_buffer, 64, bytemuck::bytes_of(&self.scene.time()));
        self.window.set_title(&format!("Voxel Raytracing -- Frame time: {:05.2}ms", dt.as_secs_f32()*1000.0));
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Main Encoder"),
        });

        {
            let mut lighting_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {label: Some("Lighting pass")});
            lighting_pass.set_pipeline(&self.lighting_compute_pipeline);
            lighting_pass.set_bind_group(0, &self.raytrace_bind_group, &[]); // TODO: remove this, it's unused
            lighting_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            lighting_pass.set_bind_group(2, &self.scene_bind_group, &[]);
            // One workgroup per chunk 
            lighting_pass.dispatch_workgroups(8, 8, 8);
        }
        {
            let mut compute_pass = encoder.begin_compute_pass(
                &wgpu::ComputePassDescriptor { label: Some("Compute pass") }
            );
            compute_pass.set_pipeline(&self.raytrace_compute_pipeline);
            compute_pass.set_bind_group(0, &self.raytrace_bind_group, &[]);
            compute_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            compute_pass.set_bind_group(2, &self.scene_bind_group, &[]);
            // Workgroup size in shader is 16, 16, 1, which means each workgroup does 16x16 pixels
            compute_pass.dispatch_workgroups(self.config.width / 15, self.config.height / 15, 1); // should use ceil_div by workgroup size instead of 15
        }

        { // scope drops render pass at the end, so we can call encoder.finish()
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color), // can probably be removed
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
                /*depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        store: true,
                        load: wgpu::LoadOp::Clear(1.0),
                    }),
                    stencil_ops: None,
                }),*/
            });

            render_pass.set_pipeline(&self.screen_render_pipeline);
            render_pass.set_bind_group(0, &self.screen_render_bind_group, &[]);
            render_pass.draw(0..6, 0..1);

        }
        
        self.queue.submit([encoder.finish()]);
        output.present();

        Ok(())
    }
}

fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    shader: wgpu::ShaderModuleDescriptor,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(shader);
    device.create_render_pipeline(
        &wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState { 
                module: &shader,
                entry_point: "vs_main",
                buffers: vertex_layouts,
            },
            fragment: Some(wgpu::FragmentState { 
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE, 
                        alpha: wgpu::BlendComponent::REPLACE, 
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })], 
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        }
    )
}

fn create_screen_bind_group(device: &wgpu::Device, texture: &texture::Texture) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
    let screen_render_bind_group_layout = device.create_bind_group_layout(
        &wgpu::BindGroupLayoutDescriptor {
            label: Some("Scren rendering bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true, },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                }
            ],     
        }
    );
    let screen_render_bind_group = device.create_bind_group(
        &wgpu::BindGroupDescriptor {
            label: Some("screen rendering bind group"),
            layout: &screen_render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        }
    );
    (screen_render_bind_group, screen_render_bind_group_layout)
}

fn create_raytrace_bind_group(device: &wgpu::Device, screen_texture: &texture::Texture, srgb_format: wgpu::TextureFormat, skybox: &texture::Texture) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
    let raytracing_bind_group_layout = device.create_bind_group_layout(
        &wgpu::BindGroupLayoutDescriptor { 
            label: Some("raytracing_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture { 
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: srgb_format,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture { 
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                }
            ],
        }
    );
    let raytrace_bind_group = device.create_bind_group(
        &wgpu::BindGroupDescriptor {
            label: Some("raytracing bind group"),
            layout: &raytracing_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&screen_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&skybox.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&skybox.sampler),
                },
            ],
        }
    );
    (raytrace_bind_group, raytracing_bind_group_layout)
}

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub async fn run() {
    cfg_if::cfg_if!{
        if #[cfg(target_arch="wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Info).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().with_title("Voxel Raytracing").build(&event_loop).unwrap();
    let mut state = State::new(window).await;
    let mut last_render_time = instant::Instant::now();

    #[cfg(target_arch = "wasm32")]
    {
        // Winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        window.set_inner_size(PhysicalSize::new(450, 400));
        
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("wasm-example")?;
                let canvas = web_sys::Element::from(window.canvas());
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::RedrawRequested(window_id) if window_id == state.window().id() => {
                let now = instant::Instant::now();
                let dt = now - last_render_time;
                last_render_time = now;
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if lost
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    // The system is out of memory, we should probably quit
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(e) => eprintln!("{:?}", e),
                }
                state.update(dt);
            }
            Event::MainEventsCleared => {
                // RedrawRequested will only trigger once, unless we manually
                // request it.
                state.window().request_redraw();
            }
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta },.. } => {
                if state.is_mouse_pressed {
                    state.camera.controller.process_mouse(delta.0, delta.1)
                }
            }
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == state.window().id() => if !state.input(event) {
                match event {
                    #[cfg(not(target_arch="wasm32"))]
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        // new_inner_size is &&mut so we have to dereference it twice
                        state.resize(**new_inner_size);
                    }
                    _ => {}
                }
            },
            _ => {}
        }
    });
}