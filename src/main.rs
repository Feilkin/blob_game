use crate::camera::PanOrbitCamera;
use crate::raymarching::Blob;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::math::Vec3Swizzles;
use bevy::pbr::CascadeShadowConfigBuilder;
use bevy::{
    core_pipeline::tonemapping::Tonemapping, diagnostic::FrameTimeDiagnosticsPlugin, math::vec3,
    prelude::*, render::renderer::RenderDevice, window::CursorGrabMode,
};
use bevy_easings::Lerp;
use bevy_egui::EguiPlugin;
use smooth_bevy_cameras::controllers::orbit::{
    OrbitCameraBundle, OrbitCameraController, OrbitCameraPlugin,
};
use smooth_bevy_cameras::{LookTransform, LookTransformPlugin, Smoother};

mod bvh;
mod camera;
mod raymarching;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_linear())
                .set(AssetPlugin {
                    watch_for_changes: true,
                    ..Default::default()
                }),
        )
        .insert_resource(Msaa::Off)
        .add_plugin(LookTransformPlugin)
        .add_plugin(camera::CameraPlugin)
        .add_plugin(EguiPlugin)
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(bevy_fps_window::FpsWindowPlugin)
        .add_plugin(raymarching::RaymarchingPlugin)
        .add_plugin(bevy_mod_gizmos::GizmosPlugin)
        .add_plugin(bvh::BvhPlugin)
        .add_startup_system(setup)
        // .add_startup_system(print_render_limits)
        // .add_system(draw_debug_gizmos)
        .add_system(handle_player_input)
        .add_system(follow_player)
        .run();
}

fn print_render_limits(dev: Res<RenderDevice>) {
    println!("{:#?}", dev.limits());
}

fn draw_debug_gizmos() {
    bevy_mod_gizmos::draw_closed_line(vec![Vec3::ZERO, Vec3::X * 3.], Color::RED);
    bevy_mod_gizmos::draw_closed_line(vec![Vec3::ZERO, Vec3::Y * 3.], Color::GREEN);
    bevy_mod_gizmos::draw_closed_line(vec![Vec3::ZERO, Vec3::Z * 3.], Color::BLUE);
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // directional 'sun' light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 30000.,
            // shadows_enabled: true,
            ..default()
        },
        transform: Transform {
            translation: Vec3::new(0.0, 0.0, 4.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::PI / 4.)
                * Quat::from_rotation_z(1.13),
            ..default()
        },
        // The default cascade config is designed to handle large scenes.
        // As this example has a much smaller world, we can tighten the shadow
        // bounds for better visual quality.
        // cascade_shadow_config: CascadeShadowConfigBuilder {
        //     first_cascade_far_bound: 4.0,
        //     maximum_distance: 10.0,
        //     ..default()
        // }
        // .into(),
        ..default()
    });

    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                hdr: true,
                ..default()
            },
            tonemapping: Tonemapping::BlenderFilmic,
            transform: Transform::from_xyz(0.0, 12., 6.0)
                .looking_at(Vec3::new(0., 0., 1.), Vec3::Z),
            ..default()
        },
        DepthPrepass::default(),
        NormalPrepass::default(),
        // camera::PanOrbitCamera {
        //     radius: 3.0,
        //     focus: vec3(0.0, 0.0, 1.0),
        //     ..default()
        // },
        LookTransform::new(vec3(0., -7., 5.), Vec3::ZERO, Vec3::Z),
        Smoother::new(0.6),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/diffuse (1).ktx2"),
            specular_map: asset_server.load("environment_maps/specular (1).ktx2"),
        },
    ));

    commands.spawn(SceneBundle {
        scene: asset_server.load("petri.glb#Scene0"),
        ..default()
    });
}

#[derive(Component)]
pub struct PlayerInput;

fn handle_player_input(
    mut player_blob: Query<(&mut Transform, &mut Blob), With<PlayerInput>>,
    keys: Res<Input<KeyCode>>,
    time: Res<Time>,
) {
    for (mut transform, mut blob) in player_blob.iter_mut() {
        let mut move_vector = Vec3::ZERO;
        move_vector.y = -1.0;

        let mut direction = blob.direction;

        // if keys.pressed(KeyCode::W) {
        //     move_vector.y = 1.0;
        // }
        // if keys.pressed(KeyCode::S) {
        //     move_vector.y = -1.0;
        // }
        // if keys.pressed(KeyCode::D) {
        //     move_vector.x = 1.0;
        // }
        // if keys.pressed(KeyCode::A) {
        //     move_vector.x = -1.0;
        // }
        if keys.pressed(KeyCode::A) {
            direction += 1.0 * 2.0 * time.delta_seconds();
        }
        if keys.pressed(KeyCode::D) {
            direction += -1.0 * 2.0 * time.delta_seconds();
        }

        // if move_vector.length() == 0.0 {
        //     continue;
        // }

        blob.direction = direction;

        transform.translation +=
            Quat::from_rotation_z(direction) * move_vector.normalize() * 3.1 * time.delta_seconds();

        let transform_length = transform.translation.xy().length();
        let play_area_size = 9.8;
        if transform_length > play_area_size - blob.size * 0.33 {
            let direction_to_center = -transform.translation.xy().normalize();
            transform.translation += (direction_to_center
                * (transform_length - play_area_size + blob.size * 0.33))
                .extend(0.0);
        }
    }
}

fn follow_player(
    mut cameras: Query<&mut LookTransform>,
    player_blobs: Query<(&Transform, &Blob), With<PlayerInput>>,
) {
    let camera_offset = vec3(0., -7., 6.);

    for (transform, blob) in player_blobs.iter() {
        for mut camera in cameras.iter_mut() {
            let camera_offset_rotated =
                Quat::from_rotation_z(blob.direction + std::f32::consts::PI) * camera_offset;
            camera.eye = transform.translation + camera_offset_rotated;
            camera.target = transform.translation;
        }
    }
}
