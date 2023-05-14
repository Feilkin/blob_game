//! Pan orbit camera
use bevy::core_pipeline::clear_color::ClearColorConfig;
use bevy::core_pipeline::core_3d::Camera3dDepthLoadOp;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::camera::Projection;
use bevy_egui::{egui, EguiContext, EguiContexts};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(pan_orbit_camera).add_system(fov_slider);
    }
}

/// Tags an entity as capable of panning and orbiting.
#[derive(Component)]
pub struct PanOrbitCamera {
    /// The "focus point" to orbit around. It is automatically updated when panning the camera
    pub focus: Vec3,
    pub radius: f32,
    pub upside_down: bool,
    pub auto_rotate: bool,
}

impl Default for PanOrbitCamera {
    fn default() -> Self {
        PanOrbitCamera {
            focus: Vec3::ZERO,
            radius: 5.0,
            upside_down: false,
            auto_rotate: false,
        }
    }
}

fn fov_slider(
    mut query: Query<(&mut Projection, &mut PanOrbitCamera)>,
    mut egui_contexts: EguiContexts,
) {
    egui::Window::new("Camera").show(egui_contexts.ctx_mut(), |ui| {
        for (mut projection, mut pan_orbit) in query.iter_mut() {
            if let Projection::Perspective(ref mut pers) = &mut *projection {
                let mut temp = pers.fov.to_degrees();
                ui.add(egui::Slider::new(&mut temp, 10.0..=180.0));
                pers.fov = temp.to_radians();
            }

            ui.add(egui::Checkbox::new(
                &mut pan_orbit.auto_rotate,
                "Auto rotate",
            ));
        }
    });
}

/// Pan the camera with middle mouse click, zoom with scroll wheel, orbit with right mouse click.
fn pan_orbit_camera(
    windows: Query<&Window>,
    mut ev_motion: EventReader<MouseMotion>,
    mut ev_scroll: EventReader<MouseWheel>,
    input_mouse: Res<Input<MouseButton>>,
    mut query: Query<(&mut PanOrbitCamera, &mut Transform, &Projection)>,
    time: Res<Time>,
) {
    // change input mapping for orbit and panning here
    let orbit_button = MouseButton::Right;
    let pan_button = MouseButton::Middle;

    let mut pan = Vec2::ZERO;
    let mut rotation_move = Vec2::ZERO;
    let mut scroll = 0.0;
    let mut orbit_button_changed = false;

    if input_mouse.pressed(orbit_button) {
        for ev in ev_motion.iter() {
            rotation_move += ev.delta;
        }
    } else if input_mouse.pressed(pan_button) {
        // Pan only if we're not rotating at the moment
        for ev in ev_motion.iter() {
            pan += ev.delta;
        }
    }
    for ev in ev_scroll.iter() {
        scroll += ev.y;
    }
    if input_mouse.just_released(orbit_button) || input_mouse.just_pressed(orbit_button) {
        orbit_button_changed = true;
    }

    for (mut pan_orbit, mut transform, projection) in query.iter_mut() {
        if orbit_button_changed {
            // only check for upside down when orbiting started or ended this frame
            // if the camera is "upside" down, panning horizontally would be inverted, so invert the input to make it correct
            let up = transform.rotation * Vec3::Z;
            pan_orbit.upside_down = up.z <= 0.0;
        }

        if pan_orbit.auto_rotate {
            rotation_move += Vec2::new(1., 0.) * time.delta_seconds() * 5.;
        }
        let window = get_primary_window_size(windows.get_single().unwrap());

        let mut any = false;
        if rotation_move.length_squared() > 0.0 {
            any = true;

            let delta_x = {
                let delta = rotation_move.x / window.x * std::f32::consts::PI * 2.0;
                if pan_orbit.upside_down {
                    -delta
                } else {
                    delta
                }
            };
            let delta_y = rotation_move.y / window.y * std::f32::consts::PI;
            let yaw = Quat::from_rotation_z(-delta_x);
            let pitch = Quat::from_rotation_x(-delta_y);
            transform.rotation = yaw * transform.rotation; // rotate around global y axis
            transform.rotation = transform.rotation * pitch; // rotate around local x axis
        } else if pan.length_squared() > 0.0 {
            any = true;
            // make panning distance independent of resolution and FOV,
            if let Projection::Perspective(projection) = projection {
                pan *= Vec2::new(projection.fov * projection.aspect_ratio, projection.fov) / window;
            }
            // translate by local axes
            let right = transform.rotation * Vec3::X * -pan.x;
            let mut up: Vec3 = transform.rotation * Vec3::Y * pan.y;
            up.z = 0.;
            // up = up.normalize_or_zero();
            // let right = Vec3::X * pan.x;
            // let up = Vec3::Y * -pan.y;
            // make panning proportional to distance away from focus point
            let translation = (right + up) * pan_orbit.radius;
            pan_orbit.focus += translation;
        } else if scroll.abs() > 0.0 {
            any = true;
            pan_orbit.radius -= scroll * pan_orbit.radius * 0.2;
            // dont allow zoom to reach zero or you get stuck
            pan_orbit.radius = pan_orbit.radius.clamp(2.0, 175.0);
        }

        if any {
            // emulating parent/child to make the yaw/y-axis rotation behave like a turntable
            // parent = x and y rotation
            // child = z-offset
            let rot_matrix = Mat3::from_quat(transform.rotation);
            transform.translation =
                pan_orbit.focus + rot_matrix.mul_vec3(Vec3::new(0.0, 0.0, pan_orbit.radius));
        }
    }
}

fn get_primary_window_size(window: &Window) -> Vec2 {
    Vec2::new(window.width() as f32, window.height() as f32)
}
