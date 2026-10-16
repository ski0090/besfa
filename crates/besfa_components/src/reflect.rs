//! Changing components by their reflected type path, with values as JSON in
//! the shape the editor sends them. Shared with `besfa_editor_plugin`.

use bevy::{
    asset::HandleDeserializeProcessor,
    prelude::*,
    reflect::{
        ReflectMut, ReflectRef, TypeRegistration, TypeRegistry, serde::TypedReflectDeserializer,
    },
};
use serde::de::DeserializeSeed;
use serde_json::Value;

/// Sets `component` on `entity` to `value`, adding it if the entity has
/// none, which is how undoing a removal puts it back. The editor sends the
/// whole value: a field left out takes its default when the type has one.
pub fn set(
    world: &mut World,
    entity: Entity,
    component: &str,
    value: &Value,
) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (registration, reflect) = component_registration(&registry, component)?;
    let immutable = world
        .components()
        .get_id(registration.type_id())
        .and_then(|id| world.components().get_info(id))
        .is_some_and(|info| !info.mutable());
    if immutable {
        return Err(format!("{component} cannot be changed"));
    }
    let value = match world.get_resource::<AssetServer>().cloned() {
        // Handles name their asset by path, the way the scene file does;
        // changing a prefab's path loads that prefab.
        Some(assets) => {
            let mut loader = &assets;
            let mut processor = HandleDeserializeProcessor {
                load_from_path: &mut loader,
            };
            TypedReflectDeserializer::with_processor(registration, &registry, &mut processor)
                .deserialize(value)
        }
        None => TypedReflectDeserializer::new(registration, &registry).deserialize(value),
    }
    .map_err(|error| error.to_string())?;
    let mut target = entity_mut(world, entity)?;
    let result = match reflect.reflect_mut(&mut target) {
        Some(mut component) => component
            .try_apply(value.as_ref())
            .map(|()| trim_lists(component.as_partial_reflect_mut(), value.as_ref()))
            .map_err(|error| error.to_string()),
        None => {
            reflect.insert(&mut target, value.as_ref(), &registry);
            Ok(())
        }
    };
    result
}

/// Applying a value keeps the items of a list past the value's end; drop
/// them, so a shorter list replaces a longer one.
// ponytail: follows lists through structs, tuple structs and enums; maps,
// tuples and arrays of lists keep their extra items.
fn trim_lists(target: &mut dyn PartialReflect, value: &dyn PartialReflect) {
    match (target.reflect_mut(), value.reflect_ref()) {
        (ReflectMut::List(list), ReflectRef::List(value)) => {
            while list.len() > value.len() {
                list.pop();
            }
            for (index, item) in value.iter().enumerate() {
                if let Some(target) = list.get_mut(index) {
                    trim_lists(target, item);
                }
            }
        }
        (ReflectMut::Struct(fields), ReflectRef::Struct(value)) => {
            for (name, item) in value.iter_fields() {
                if let Some(target) = fields.field_mut(name) {
                    trim_lists(target, item);
                }
            }
        }
        (ReflectMut::TupleStruct(fields), ReflectRef::TupleStruct(value)) => {
            for (index, item) in value.iter_fields().enumerate() {
                if let Some(target) = fields.field_mut(index) {
                    trim_lists(target, item);
                }
            }
        }
        (ReflectMut::Enum(variant), ReflectRef::Enum(value)) => {
            for (index, item) in value.iter_fields().enumerate() {
                if let Some(target) = variant.field_at_mut(index) {
                    trim_lists(target, item.value());
                }
            }
        }
        _ => {}
    }
}

pub fn remove(world: &mut World, entity: Entity, component: &str) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (_, reflect) = component_registration(&registry, component)?;
    reflect.remove(&mut entity_mut(world, entity)?);
    Ok(())
}

/// `entity` and everything under it, the entity first.
pub fn subtree(world: &World, entity: Entity) -> Vec<Entity> {
    let mut entities = vec![entity];
    let mut next = 0;
    while let Some(&entity) = entities.get(next) {
        next += 1;
        if let Some(children) = world.get::<Children>(entity) {
            entities.extend(children.iter());
        }
    }
    entities
}

pub fn component_registration<'a>(
    registry: &'a TypeRegistry,
    path: &str,
) -> Result<(&'a TypeRegistration, &'a ReflectComponent), String> {
    let registration = registry
        .get_with_type_path(path)
        .ok_or_else(|| format!("{path} is not registered for reflection"))?;
    let reflect = registration
        .data::<ReflectComponent>()
        .ok_or_else(|| format!("{path} is not a reflected component"))?;
    Ok((registration, reflect))
}

pub fn entity_mut(world: &mut World, entity: Entity) -> Result<EntityWorldMut<'_>, String> {
    world
        .get_entity_mut(entity)
        .map_err(|_| format!("entity {entity} does not exist"))
}
