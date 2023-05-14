//! Bounding volume hierarchy
use crate::raymarching::{EntityBufferIndex, VoxelMaterial};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, RenderMaterials};
use bevy::prelude::*;
use bevy::reflect::TypeUuid;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::mesh::MeshVertexBufferLayout;
use bevy::render::render_resource::{
    AsBindGroup, OwnedBindingResource, RenderPipelineDescriptor, ShaderRef, ShaderType,
    SpecializedMeshPipelineError, StorageBuffer,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::{extract_resource::ExtractResource, Extract, RenderApp, RenderSet};
use bevy_mod_gizmos::draw_gizmos_with_line;

#[derive(Component)]
pub struct CalculateBvh;

/// Bounding box in model space (not rotated, not translated)
#[derive(Component)]
pub struct LocalBoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

/// Axis-aligned bounding box in world space
#[derive(Component, Copy, Clone, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn centroid(&self) -> Vec3 {
        self.min + (self.max - self.min) * 0.5
    }

    pub fn total_surface_area(&self) -> f32 {
        let extents = self.max - self.min;
        return extents.x * extents.y * 2.
            + extents.x * extents.z * 2.
            + extents.y * extents.z * 2.;
    }
}

#[derive(Clone, ExtractResource, Resource)]
pub struct BvhTree {
    root: BvhNode,
}

impl Default for BvhTree {
    fn default() -> Self {
        BvhTree {
            root: BvhNode {
                aabb: Aabb {
                    min: Default::default(),
                    max: Default::default(),
                },
                kind: BvhNodeKind::Leaf(Entity::from_raw(0)),
            },
        }
    }
}

#[derive(Clone)]
pub struct BvhNode {
    aabb: Aabb,
    kind: BvhNodeKind,
}

#[derive(Clone)]
pub enum BvhNodeKind {
    Leaf(Entity),
    Branch(Box<BvhNode>, Box<BvhNode>),
}

#[derive(Resource)]
pub struct BvhBuffer(pub StorageBuffer<GpuTree>);

pub struct BvhPlugin;

impl Plugin for BvhPlugin {
    fn build(&self, app: &mut App) {
        app
            // .add_plugin(ExtractResourcePlugin::<BvhTree>::default())
            // .add_startup_system(setup_bvh)
            .add_system(update_bvh_aabb)
            .insert_resource(BvhTree::default())
            .add_system(update_bvh)
            .add_system(update_bvh_buffer.after(update_bvh))
            .add_system(update_material_buffer.in_base_set(CoreSet::PostUpdate));
        // .add_system(update_bvh_debug_mesh)

        // let render_app = app.sub_app_mut(RenderApp);
        // render_app
        //     .insert_resource(BvhTree::default())
        //     .add_system(extract_aabb.in_schedule(ExtractSchedule))
        //     .add_system(update_bvh.in_set(RenderSet::Prepare))
        //     .add_system(
        //         update_bvh_buffer
        //             .in_set(RenderSet::Prepare)
        //             .after(update_bvh),
        //     )
        //     .add_system(update_material_buffer.in_set(RenderSet::PhaseSort));
    }
}

fn update_material_buffer(
    instances: Query<&Handle<VoxelMaterial>>,
    mut mats: ResMut<Assets<VoxelMaterial>>,
    bvh: Res<BvhBuffer>,
) {
    if let Some(buffer) = bvh.0.buffer() {
        for instance in instances.iter() {
            let material = mats.get_mut(instance).unwrap();
            material.bvh = buffer.clone();
        }
    }
}

#[derive(Debug, Clone, ShaderType)]
pub struct GpuNode {
    /// Minimum of the AABB
    min: Vec3,
    /// Maximum of the AABB
    max: Vec3,
    /// Left child index, or -1 if leaf node
    left: i32,
    /// Right child index, or entity index if leaf node
    right: i32,
}

#[derive(Debug, Clone, ShaderType)]
pub struct GpuTree {
    #[size(runtime)]
    tree: Vec<GpuNode>,
}

fn extract_aabb(
    mut commands: Commands,
    entities: Extract<Query<(Entity, &Aabb), With<CalculateBvh>>>,
) {
    let mut values = Vec::new();

    for (entity, aabb) in entities.iter() {
        values.push((entity, (aabb.clone(), CalculateBvh)));
    }
    commands.insert_or_spawn_batch(values);
}

fn update_bvh_aabb(
    mut query: Query<
        (Entity, &LocalBoundingBox, &Transform, Option<&mut Aabb>),
        (
            With<CalculateBvh>,
            Or<(Changed<Transform>, Changed<LocalBoundingBox>)>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, local_bb, transform, maybe_aabb) in query.iter_mut() {
        let local_bb: &LocalBoundingBox = local_bb;
        let transform: &Transform = transform;
        let maybe_aabb: Option<Mut<Aabb>> = maybe_aabb;

        // TODO: rotation
        let new_aabb = &local_bb.into() * transform.scale + transform.translation;
        if let Some(mut aabb) = maybe_aabb {
            *aabb = new_aabb
        } else {
            commands.entity(entity).insert(new_aabb);
        }
    }
}

fn update_bvh(
    mut commands: Commands,
    objects: Query<(Entity, &Aabb), With<CalculateBvh>>,
    mut entities: Local<Vec<(Entity, Aabb)>>,
    mut finished: Local<bool>,
) {
    entities.clear();
    // collect all entities
    for (entity, aabb) in objects.iter() {
        entities.push((entity, aabb.clone()));
    }

    if entities.is_empty() {
        println!("no entities for BVH");
        return;
    }

    // make root node
    let root = split_node(&mut entities);

    // if let BvhNodeKind::Branch(left, right) = &root.kind {
    //     spawn_debug_cubes(&mut commands, left);
    //     spawn_debug_cubes(&mut commands, right);
    // }

    commands
        .spawn(TransformBundle::from_transform(
            Transform::from_translation(Vec3::new(0., 0., 0.)),
        ))
        .insert((root.aabb.clone(),));

    commands.insert_resource(BvhTree { root });
    *finished = true;
}

fn update_bvh_buffer(
    mut commands: Commands,
    tree: Res<BvhTree>,
    entity_to_index: Query<&EntityBufferIndex>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let mut nodes = Vec::new();

    push_node_to_buffer(&tree.root, &mut nodes, &entity_to_index);

    let gpu_tree = GpuTree { tree: nodes };

    let mut buffer = StorageBuffer::from(gpu_tree);
    buffer.write_buffer(&render_device, &render_queue);

    commands.insert_resource(BvhBuffer(buffer));
}

fn push_node_to_buffer(
    node: &BvhNode,
    buffer: &mut Vec<GpuNode>,
    entity_to_index: &Query<&EntityBufferIndex>,
) {
    match &node.kind {
        BvhNodeKind::Leaf(entity) => buffer.push(GpuNode {
            min: node.aabb.min,
            max: node.aabb.max,
            left: -1,
            right: entity_to_index
                .get(*entity)
                .unwrap_or(&EntityBufferIndex(-1))
                .0,
        }),
        BvhNodeKind::Branch(left, right) => {
            let own_index = buffer.len();
            buffer.push(GpuNode {
                min: node.aabb.min,
                max: node.aabb.max,
                left: 0,
                right: 0,
            });

            let left_index = buffer.len();
            push_node_to_buffer(left, buffer, &entity_to_index);

            let right_index = buffer.len();
            push_node_to_buffer(right, buffer, &entity_to_index);

            buffer[own_index].left = left_index as i32;
            buffer[own_index].right = right_index as i32;
        }
    }
}

fn split_node(aabbs: &mut [(Entity, Aabb)]) -> BvhNode {
    assert!(aabbs.len() > 0);

    if aabbs.len() == 1 {
        return BvhNode {
            aabb: aabbs[0].1,
            kind: BvhNodeKind::Leaf(aabbs[0].0),
        };
    }

    let x_index_and_cost = {
        aabbs.sort_by(|a, b| a.1.centroid().x.total_cmp(&b.1.centroid().x));
        find_split_index_and_cost(&aabbs)
    };
    let y_index_and_cost = {
        aabbs.sort_by(|a, b| a.1.centroid().y.total_cmp(&b.1.centroid().y));
        find_split_index_and_cost(&aabbs)
    };
    let z_index_and_cost = {
        aabbs.sort_by(|a, b| a.1.centroid().z.total_cmp(&b.1.centroid().z));
        find_split_index_and_cost(&aabbs)
    };

    let (left, right) =
        if x_index_and_cost.1 < y_index_and_cost.1 && x_index_and_cost.1 < z_index_and_cost.1 {
            aabbs.sort_by(|a, b| a.1.centroid().x.total_cmp(&b.1.centroid().x));
            aabbs.split_at_mut(x_index_and_cost.0)
        } else if y_index_and_cost.1 < z_index_and_cost.1 {
            aabbs.sort_by(|a, b| a.1.centroid().y.total_cmp(&b.1.centroid().y));
            aabbs.split_at_mut(y_index_and_cost.0)
        } else {
            aabbs.split_at_mut(z_index_and_cost.0)
        };

    let left_node = split_node(left);
    let right_node = split_node(right);

    BvhNode {
        aabb: merge_aabbs(aabbs),
        kind: BvhNodeKind::Branch(Box::new(left_node), Box::new(right_node)),
    }
}

fn find_split_index_and_cost(aabbs: &[(Entity, Aabb)]) -> (usize, f32) {
    assert!(aabbs.len() > 1);
    let mut min = (1, f32::INFINITY);

    for i in 1..aabbs.len() {
        let current_cost = cost(aabbs, i);
        if current_cost < min.1 {
            min = (i, current_cost);
        }
    }

    min
}

fn cost(aabbs: &[(Entity, Aabb)], index: usize) -> f32 {
    let (left, right) = aabbs.split_at(index);

    merge_aabbs(left).total_surface_area() * (index as f32)
        + merge_aabbs(right).total_surface_area() * (aabbs.len() - index) as f32
}

fn merge_aabbs(aabbs: &[(Entity, Aabb)]) -> Aabb {
    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(-f32::INFINITY, -f32::INFINITY, -f32::INFINITY);

    for aabb in aabbs {
        min.x = min.x.min(aabb.1.min.x.min(aabb.1.max.x));
        min.y = min.y.min(aabb.1.min.y.min(aabb.1.max.y));
        min.z = min.z.min(aabb.1.min.z.min(aabb.1.max.z));
        max.x = max.x.max(aabb.1.min.x.max(aabb.1.max.x));
        max.y = max.y.max(aabb.1.min.y.max(aabb.1.max.y));
        max.z = max.z.max(aabb.1.min.z.max(aabb.1.max.z));
    }

    assert_ne!(min.length(), f32::INFINITY);
    assert_ne!(max.length(), f32::INFINITY);

    return Aabb { min, max };
}

impl From<&LocalBoundingBox> for Aabb {
    fn from(local_bb: &LocalBoundingBox) -> Self {
        Aabb {
            min: local_bb.min,
            max: local_bb.max,
        }
    }
}

impl std::ops::Sub<Vec3> for &Aabb {
    type Output = Aabb;

    fn sub(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min - rhs,
            max: self.max - rhs,
        }
    }
}

impl std::ops::Sub<Vec3> for Aabb {
    type Output = Aabb;

    fn sub(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min - rhs,
            max: self.max - rhs,
        }
    }
}

impl std::ops::Add<Vec3> for &Aabb {
    type Output = Aabb;

    fn add(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min + rhs,
            max: self.max + rhs,
        }
    }
}

impl std::ops::Add<Vec3> for Aabb {
    type Output = Aabb;

    fn add(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min + rhs,
            max: self.max + rhs,
        }
    }
}

impl std::ops::Mul<Vec3> for &Aabb {
    type Output = Aabb;

    fn mul(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min * rhs,
            max: self.max * rhs,
        }
    }
}

impl std::ops::Mul<Vec3> for Aabb {
    type Output = Aabb;

    fn mul(self, rhs: Vec3) -> Self::Output {
        Aabb {
            min: self.min * rhs,
            max: self.max * rhs,
        }
    }
}
