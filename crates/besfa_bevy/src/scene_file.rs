use bevy::prelude::*;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) const DEFAULT_SCENE_FILE_NAME: &str = "Scene.besfa.json";
const DEFAULT_SCENE_JSON: &str = include_str!("../../../Scene.besfa.json");

#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct PreviewSceneSource {
    pub(crate) path: Option<PathBuf>,
}

impl PreviewSceneSource {
    pub(crate) fn from_path(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub(crate) fn set_project_path(&mut self, project_path: impl AsRef<Path>) {
        let project_path = project_path.as_ref();
        self.path = Some(if project_path.is_dir() {
            project_path.join(DEFAULT_SCENE_FILE_NAME)
        } else {
            project_path.to_path_buf()
        });
    }

    fn resolved_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(DEFAULT_SCENE_FILE_NAME)
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedSceneFile {
    pub(crate) scene: SceneFile,
    pub(crate) status: SceneLoadStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SceneLoadStatus {
    Loaded(PathBuf),
    BuiltInFallback,
    FileFallback { path: PathBuf, error: String },
}

impl SceneLoadStatus {
    pub(crate) fn log_message(&self) -> String {
        match self {
            SceneLoadStatus::Loaded(path) => {
                format!("Loaded scene file {}", path.display())
            }
            SceneLoadStatus::BuiltInFallback => "Loaded built-in preview scene".to_string(),
            SceneLoadStatus::FileFallback { path, error } => {
                format!(
                    "Scene file {} could not be loaded ({error}); using built-in preview scene",
                    path.display()
                )
            }
        }
    }

    pub(crate) fn log_level(&self) -> &'static str {
        match self {
            SceneLoadStatus::FileFallback { .. } => "warn",
            SceneLoadStatus::Loaded(_) | SceneLoadStatus::BuiltInFallback => "info",
        }
    }
}

pub(crate) fn load_scene_file(source: &PreviewSceneSource) -> LoadedSceneFile {
    let path = source.resolved_path();
    if path.is_file() {
        return match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| SceneFile::from_json(&text))
        {
            Ok(scene) => LoadedSceneFile {
                scene,
                status: SceneLoadStatus::Loaded(path),
            },
            Err(error) => LoadedSceneFile {
                scene: default_scene_file(),
                status: SceneLoadStatus::FileFallback { path, error },
            },
        };
    }

    LoadedSceneFile {
        scene: default_scene_file(),
        status: SceneLoadStatus::BuiltInFallback,
    }
}

fn default_scene_file() -> SceneFile {
    SceneFile::from_json(DEFAULT_SCENE_JSON).unwrap_or_default()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SceneFile {
    pub(crate) version: u32,
    pub(crate) entities: Vec<SceneEntityDefinition>,
}

impl SceneFile {
    fn from_json(text: &str) -> Result<Self, String> {
        let scene = serde_json::from_str::<Self>(text).map_err(|error| error.to_string())?;
        if scene.version != 1 {
            return Err(format!("unsupported Scene version {}", scene.version));
        }

        Ok(scene)
    }
}

impl Default for SceneFile {
    fn default() -> Self {
        Self {
            version: 1,
            entities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SceneEntityDefinition {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) transform: SceneTransformDefinition,
    pub(crate) mesh: Option<SceneMeshDefinition>,
    pub(crate) material: SceneMaterialDefinition,
    pub(crate) light: Option<ScenePointLightDefinition>,
    pub(crate) camera: Option<SceneCameraDefinition>,
    pub(crate) pick_half_extents: Option<SceneVec3>,
    pub(crate) spin_y_radians_per_second: Option<f32>,
}

impl Default for SceneEntityDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Entity".to_string(),
            kind: "entity".to_string(),
            parent_id: None,
            transform: SceneTransformDefinition::default(),
            mesh: None,
            material: SceneMaterialDefinition::default(),
            light: None,
            camera: None,
            pick_half_extents: None,
            spin_y_radians_per_second: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SceneTransformDefinition {
    pub(crate) translation: SceneVec3,
    pub(crate) rotation_y_radians: Option<f32>,
    pub(crate) look_at: Option<SceneVec3>,
}

impl SceneTransformDefinition {
    pub(crate) fn to_transform(self) -> Transform {
        let translation = self.translation.to_vec3();
        let mut transform = Transform::from_translation(translation);
        if let Some(target) = self.look_at {
            transform = transform.looking_at(target.to_vec3(), Vec3::Y);
        } else if let Some(rotation_y_radians) = self.rotation_y_radians {
            transform.rotation = Quat::from_rotation_y(rotation_y_radians);
        }

        transform
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SceneMeshDefinition {
    pub(crate) primitive: SceneMeshPrimitive,
    pub(crate) size: SceneVec3,
}

impl Default for SceneMeshDefinition {
    fn default() -> Self {
        Self {
            primitive: SceneMeshPrimitive::Cube,
            size: SceneVec3::splat(1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SceneMeshPrimitive {
    Plane,
    #[default]
    Cube,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub(crate) struct SceneMaterialDefinition {
    pub(crate) base_color: SceneColor,
    pub(crate) metallic: f32,
    pub(crate) perceptual_roughness: f32,
}

impl Default for SceneMaterialDefinition {
    fn default() -> Self {
        Self {
            base_color: SceneColor::new(0.35, 0.47, 0.95),
            metallic: 0.05,
            perceptual_roughness: 0.6,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub(crate) struct SceneColor {
    pub(crate) r: f32,
    pub(crate) g: f32,
    pub(crate) b: f32,
}

impl SceneColor {
    pub(crate) const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub(crate) fn to_color(self) -> Color {
        Color::srgb(self.r, self.g, self.b)
    }
}

impl Default for SceneColor {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub(crate) struct ScenePointLightDefinition {
    pub(crate) intensity: f32,
    pub(crate) range: f32,
    pub(crate) shadows_enabled: bool,
}

impl Default for ScenePointLightDefinition {
    fn default() -> Self {
        Self {
            intensity: 800_000.0,
            range: 20.0,
            shadows_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SceneCameraDefinition {}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SceneVec3 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
}

impl SceneVec3 {
    pub(crate) const fn splat(value: f32) -> Self {
        Self {
            x: value,
            y: value,
            z: value,
        }
    }

    pub(crate) fn to_vec3(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}
