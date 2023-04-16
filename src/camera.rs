use glam::{Mat4, Vec3};
use winit::event::*;
use winit::dpi::PhysicalPosition;
use instant::Duration;
use std::f32::consts::FRAC_PI_2;

const SAFE_FRAC_PI_2: f32 = FRAC_PI_2 - 0.0001;


pub struct Camera {
    pub projection: Projection,
    pub view: View,
    pub controller: CameraController,
}


impl Camera {
    pub fn new(pos: Vec3, yaw: f32, pitch: f32, aspect: f32, fov: f32, znear: f32, zfar: f32,) -> Self {
        let view = View::new(pos, yaw, pitch);
        let projection = Projection::new(aspect, fov, znear, zfar);
        let controller = CameraController::new(4.0, 1.0);
        Self {
            projection,
            view,
            controller,
        }
    }
    pub fn uniform(&self) -> CameraUniform {
        CameraUniform::from_view_proj(&self.view, &self.projection)
    }

    pub fn update(&mut self, dt: Duration) {
        self.controller.update_camera(&mut self.view, dt);
    }
    
}

pub struct View {
    pub position: Vec3,
    yaw: f32,
    pitch: f32,
}

impl View {
    pub fn new<
        V: Into<Vec3>,
        Y: Into<f32>,
        P: Into<f32>,
    >(
        position: V,
        yaw: Y,
        pitch: P,
    ) -> Self {
        Self {
            position: position.into(),
            yaw: yaw.into(), 
            pitch: pitch.into(),
        }
    }
    pub fn calc_matrix(&self) -> Mat4 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Mat4::look_to_lh(
            self.position,
            Vec3::new(
                cos_pitch * cos_yaw,
                sin_pitch,
                cos_pitch * sin_yaw
            ).normalize(),
            Vec3::Y,
        )
    }
}

pub struct Projection {
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

impl Projection {
    pub fn new(
        aspect: f32,
        fovy: f32,
        znear: f32,
        zfar: f32,
    ) -> Self {
        Self {
            aspect,
            fovy,
            znear, 
            zfar
        }
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }
    pub fn calc_matrix(&self) -> Mat4 {
        Mat4::perspective_lh(self.fovy, self.aspect, self.znear, self.zfar)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    position: [f32;4],
    inv_view: [[f32;4];4],
    inv_proj: [[f32;4];4],
}

impl CameraUniform {
    pub fn from_view_proj(view: &View, proj: &Projection) -> Self {
        Self { 
            position: view.position.extend(0.0).to_array(), 
            inv_view: view.calc_matrix().inverse().to_cols_array_2d(), 
            inv_proj: proj.calc_matrix().inverse().to_cols_array_2d(),
        }
    }
}


#[derive(Debug)]
pub struct CameraController {
    amount_left: f32,
    amount_right: f32,
    amount_forward: f32,
    amount_backward: f32,
    amount_up: f32,
    amount_down: f32,
    rotate_horizontal: f32,
    rotate_vertical: f32,
    scroll: f32,
    speed: f32,
    sensitivity: f32,
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            amount_left: 0.0,
            amount_right: 0.0,
            amount_forward: 0.0,
            amount_backward: 0.0,
            amount_up: 0.0,
            amount_down: 0.0,
            rotate_horizontal: 0.0,
            rotate_vertical: 0.0,
            scroll: 0.0,
            speed,
            sensitivity,
        }
    }

    pub fn process_keyboard(&mut self, key: VirtualKeyCode, state: ElementState) -> bool{
        let amount = if state == ElementState::Pressed { 1.0 } else { 0.0 };
        match key {
            VirtualKeyCode::W | VirtualKeyCode::Up => {
                self.amount_forward = amount;
                true
            }
            VirtualKeyCode::S | VirtualKeyCode::Down => {
                self.amount_backward = amount;
                true
            }
            VirtualKeyCode::A | VirtualKeyCode::Left => {
                self.amount_left = amount;
                true
            }
            VirtualKeyCode::D | VirtualKeyCode::Right => {
                self.amount_right = amount;
                true
            }
            VirtualKeyCode::Space => {
                self.amount_up = amount;
                true
            }
            VirtualKeyCode::LShift => {
                self.amount_down = amount;
                true
            }
            VirtualKeyCode::Q => { // quick mouseless rotation, may need to be tuned
                self.rotate_horizontal = amount * 0.5;
                false
            }
            VirtualKeyCode::E => {
                self.rotate_horizontal = -amount * 0.5;
                false
            }
            _ => false,
        }
    }

    pub fn process_mouse(&mut self, mouse_dx: f64, mouse_dy: f64) {
        self.rotate_horizontal = -mouse_dx as f32;
        self.rotate_vertical = -mouse_dy as f32;
    }

    pub fn process_scroll(&mut self, delta: &MouseScrollDelta) {
        self.scroll = -match delta {
            // I'm assuming a line is about 100 pixels
            MouseScrollDelta::LineDelta(_, scroll) => scroll * 100.0,
            MouseScrollDelta::PixelDelta(PhysicalPosition {
                y: scroll,
                ..
            }) => *scroll as f32,
        };
    }

    pub fn update_camera(&mut self, camera: &mut View, dt: Duration) {
        let dt = dt.as_secs_f32();

        //eprintln!("Pos: {} Yaw:{} Pitch: {}", camera.position, camera.yaw.to_degrees(), camera.pitch.to_degrees());
        // Move forward/backward and left/right
        let (yaw_sin, yaw_cos) = camera.yaw.sin_cos();
        let forward = Vec3::new(yaw_cos, 0.0, yaw_sin).normalize();
        //eprintln!("Facing: ({},{},{})", forward.x, forward.y, forward.z);
        let right = -Vec3::new(-yaw_sin, 0.0, yaw_cos).normalize();
        camera.position += forward * (self.amount_forward - self.amount_backward) * self.speed * dt;
        camera.position += right * (self.amount_right - self.amount_left) * self.speed * dt;

        // Move in/out (aka. "zoom")
        // Note: this isn't an actual zoom. The camera's position
        // changes when zooming. I've added this to make it easier
        // to get closer to an object you want to focus on.
        let (pitch_sin, pitch_cos) = camera.pitch.sin_cos();
        let scrollward = Vec3::new(pitch_cos * yaw_cos, pitch_sin, pitch_cos * yaw_sin).normalize();
        camera.position += scrollward * self.scroll * self.speed * self.sensitivity * dt;
        self.scroll = 0.0;

        // Move up/down. Since we don't use roll, we can just
        // modify the y coordinate directly.
        camera.position.y += (self.amount_up - self.amount_down) * self.speed * dt;

        // Rotate
        camera.yaw += self.rotate_horizontal * self.sensitivity * dt;
        camera.pitch += self.rotate_vertical * self.sensitivity * dt;

        // If process_mouse isn't called every frame, these values
        // will not get set to zero, and the camera will rotate
        // when moving in a non cardinal direction.
        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;

        // Keep the camera's angle from going too high/low.
        if camera.pitch < -SAFE_FRAC_PI_2 {
            camera.pitch = -SAFE_FRAC_PI_2;
        } else if camera.pitch > SAFE_FRAC_PI_2 {
            camera.pitch = SAFE_FRAC_PI_2;
        }
    }
}

 
