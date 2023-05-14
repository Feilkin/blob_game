//! Raymarching for bevy
use crate::bvh::CalculateBvh;
use crate::bvh::LocalBoundingBox;
use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::math::{vec3, vec4, Vec3Swizzles};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayout;
use bevy::render::render_phase::AddRenderCommand;
use bevy::render::render_resource::{
    Buffer, BufferDescriptor, BufferUsages, DynamicStorageBuffer, Extent3d,
    RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, SpecializedMeshPipelines,
    StorageBuffer, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::BevyDefault;
use bevy::render::RenderApp;
use bevy::{
    reflect::TypeUuid,
    render::render_resource::{AsBindGroup, ShaderRef},
};

pub struct RaymarchingPlugin;

impl Plugin for RaymarchingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(MaterialPlugin::<VoxelMaterial> {
            prepass_enabled: false,
            ..default()
        })
        .add_startup_system(spawn_debug_voxel)
        .add_system(update_material)
        .add_system(blob_merger);
    }
}

fn spawn_debug_voxel(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    render_device: Res<RenderDevice>,
) {
    let empty_buffer = render_device.create_buffer(&BufferDescriptor {
        label: None,
        size: 48,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let material = materials.add(VoxelMaterial {
        blobs: BlobData::default(),
        bvh: empty_buffer,
    });

    for x_ in 0..4 {
        for y_ in 0..4 {
            let x = (x_ as f32) * 2. - 4.0;
            let y = (y_ as f32) * 2. - 4.0;

            let mut e = commands.spawn((
                MaterialMeshBundle {
                    mesh: meshes.add(Mesh::from(shape::Cube { size: 2.0 })),
                    transform: Transform::from_xyz(x, y, 1.0).with_scale(vec3(1., 1., 1.)),
                    material: material.clone(),
                    ..default()
                },
                NotShadowCaster,
                Blob::default(),
                CalculateBvh,
                LocalBoundingBox {
                    min: vec3(-1., -1., -1.),
                    max: vec3(1., 1., 1.),
                },
            ));

            if x_ == 0 && y_ == 0 {
                e.insert((crate::PlayerInput));
            }
        }
    }

    commands.insert_resource(BlobMaterial(material));
}

#[derive(Component)]
pub struct Blob {
    pub size: f32,
    pub direction: f32,
    pub last_ate: f32,
}

impl Default for Blob {
    fn default() -> Self {
        Blob {
            size: 0.5,
            direction: 0.0,
            last_ate: 0.0,
        }
    }
}

fn update_material(
    mut commands: Commands,
    blobs: Query<(Entity, &Transform, &Blob)>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    material: Res<BlobMaterial>,
) {
    if let Some(instance) = materials.get_mut(&material.0) {
        instance.blobs.clear();

        for (e, transform, blob) in blobs.iter() {
            let transform: &Transform = transform;
            let blob: &Blob = blob;

            let buffer_index = instance.blobs.push(BlobEntity {
                position: transform.translation.xy(),
                size: blob.size,
                direction: blob.direction,
                last_ate: blob.last_ate,
                color: Default::default(),
            });

            commands.entity(e).insert((EntityBufferIndex(buffer_index)));
        }
    }
}

#[derive(Debug, Resource)]
struct BlobMaterial(Handle<VoxelMaterial>);

#[derive(Debug, Component)]
pub struct EntityBufferIndex(pub i32);

#[derive(Debug, Default, Clone, Copy, ShaderType)]
struct BlobEntity {
    position: Vec2,
    size: f32,
    direction: f32,
    last_ate: f32,
    color: Vec3,
}

#[derive(ShaderType, Debug, Clone)]
struct BlobData {
    blob_count: u32,
    blobs: [BlobEntity; 64],
}

impl Default for BlobData {
    fn default() -> Self {
        BlobData {
            blob_count: 0,
            blobs: [BlobEntity::default(); 64],
        }
    }
}

impl BlobData {
    fn clear(&mut self) {
        self.blob_count = 0;
    }

    fn push(&mut self, blob: BlobEntity) -> i32 {
        assert!(self.blob_count < 63);
        let index = self.blob_count as i32;

        self.blobs[self.blob_count as usize] = blob;
        self.blob_count += 1;

        index
    }
}

#[derive(AsBindGroup, TypeUuid, Debug, Clone)]
#[uuid = "f690fdae-d598-45ab-8225-97e2a3f056e0"]
pub struct VoxelMaterial {
    #[uniform(0)]
    blobs: BlobData,
    #[storage(1, read_only, buffer)]
    pub bvh: Buffer,
}

impl Material for VoxelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_material.wgsl".into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        "shaders/voxel_raymarch.wgsl".into()
    }
}

fn blob_merger(
    mut commands: Commands,
    mut blobs: Query<(Entity, &mut Transform, &mut Blob)>,
    time: Res<Time>,
) {
    let merge_factor = 0.75;
    let gain_factor = 0.15;

    let mut combinations = blobs.iter_combinations_mut();
    while let Some([mut a, mut b]) = combinations.fetch_next() {
        if a.1.translation.distance(b.1.translation) < (a.2.size + b.2.size) * merge_factor {
            let (smaller, mut bigger) = if a.2.size > b.2.size { (b, a) } else { (a, b) };
            commands.entity(smaller.0).despawn();

            let grow_size = smaller.2.size * gain_factor;
            bigger.2.size += grow_size;
            bigger.1.scale += grow_size;
            bigger.2.last_ate = time.elapsed_seconds_wrapped();
        }
    }
}
