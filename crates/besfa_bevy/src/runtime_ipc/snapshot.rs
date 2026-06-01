use crate::preview::PreviewSceneNode;
use besfa_ipc::{SceneEntityPayload, SceneSnapshotPayload, SceneTransformPayload, Vec3Payload};
use bevy::prelude::*;

pub(super) fn build_scene_snapshot(
    scene_nodes: &Query<(&PreviewSceneNode, Option<&Transform>)>,
    selected_entity_id: Option<&str>,
) -> Option<SceneSnapshotPayload> {
    let mut records = scene_nodes
        .iter()
        .map(PreviewSceneRecord::from)
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));

    let root = records
        .iter()
        .find(|record| record.parent_id.is_none())
        .map(|record| build_scene_entity(record, &records))?;

    Some(SceneSnapshotPayload {
        root,
        selected_entity_id: selected_entity_id.map(str::to_string),
    })
}

fn build_scene_entity(
    record: &PreviewSceneRecord,
    records: &[PreviewSceneRecord],
) -> SceneEntityPayload {
    let mut children = records
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(record.id.as_str()))
        .map(|child| build_scene_entity(child, records))
        .collect::<Vec<_>>();
    children.sort_by(|left, right| left.name.cmp(&right.name));

    SceneEntityPayload {
        id: record.id.clone(),
        name: record.name.clone(),
        kind: record.kind.clone(),
        transform: record.transform.clone(),
        children,
    }
}

struct PreviewSceneRecord {
    id: String,
    name: String,
    kind: String,
    parent_id: Option<String>,
    transform: Option<SceneTransformPayload>,
}

impl From<(&PreviewSceneNode, Option<&Transform>)> for PreviewSceneRecord {
    fn from((node, transform): (&PreviewSceneNode, Option<&Transform>)) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            kind: node.kind.clone(),
            parent_id: node.parent_id.clone(),
            transform: transform.map(|transform| SceneTransformPayload {
                translation: Vec3Payload {
                    x: transform.translation.x,
                    y: transform.translation.y,
                    z: transform.translation.z,
                },
            }),
        }
    }
}
