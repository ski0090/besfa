//! The terrain tools at the top of the Inspector: which brush, how big and
//! strong, and which surface layer Paint covers the ground with. The
//! editor sends the brush to the game, which paints the selected terrain
//! with the left button in the scene view.

use besfa_protocol::{ComponentData, TerrainBrush, TerrainTool};
use eframe::egui::{self, RichText, Slider, Ui};
use serde_json::{Value, json};

use crate::panels::{MUTED, section};

pub const TERRAIN: &str = "besfa_editor_plugin::terrain::Terrain";

/// The most surface layers a terrain has.
const MAX_LAYERS: usize = 4;

const TOOLS: [(TerrainTool, &str); 5] = [
    (TerrainTool::Raise, "Raise"),
    (TerrainTool::Lower, "Lower"),
    (TerrainTool::Smooth, "Smooth"),
    (TerrainTool::Flatten, "Flatten"),
    (TerrainTool::Paint, "Paint"),
];

pub struct TerrainTools {
    /// The brush picked; none selects with the left button.
    pub tool: Option<TerrainTool>,
    /// The brush's radius in meters.
    pub size: f32,
    /// 0 to 1.
    pub strength: f32,
    pub layer: usize,
}

impl Default for TerrainTools {
    fn default() -> Self {
        Self {
            tool: None,
            size: 5.0,
            strength: 0.5,
            layer: 0,
        }
    }
}

impl TerrainTools {
    /// The brush to paint with; Shift turns Raise into Lower and back.
    /// Raise and Lower take strength in meters: half the brush's radius at
    /// full strength, so a bigger brush makes a bigger hill.
    pub fn brush(&self, shift: bool) -> Option<TerrainBrush> {
        let tool = match (self.tool?, shift) {
            (TerrainTool::Raise, true) => TerrainTool::Lower,
            (TerrainTool::Lower, true) => TerrainTool::Raise,
            (tool, _) => tool,
        };
        let strength = match tool {
            TerrainTool::Raise | TerrainTool::Lower => self.strength * self.size / 2.0,
            _ => self.strength,
        };
        Some(TerrainBrush {
            tool,
            radius: self.size,
            strength,
            layer: (tool == TerrainTool::Paint).then_some(self.layer),
        })
    }

    /// The tools for the selected terrain, whose `Terrain` component is
    /// `terrain`; returns the component's new value when a layer changed.
    /// `can_sculpt` is false for a plugin too old for the brush.
    pub fn ui(&mut self, ui: &mut Ui, terrain: &ComponentData, can_sculpt: bool) -> Option<Value> {
        section(ui, "Terrain");
        if !can_sculpt {
            ui.label(
                RichText::new("Update besfa_editor_plugin in the game to sculpt terrain.")
                    .color(MUTED),
            );
            return None;
        }
        ui.horizontal_wrapped(|ui| {
            for (tool, label) in TOOLS {
                let picked = self.tool == Some(tool);
                if ui.add(egui::Button::selectable(picked, label)).clicked() {
                    self.tool = (!picked).then_some(tool);
                }
            }
        });
        ui.add(
            Slider::new(&mut self.size, 0.5..=50.0)
                .logarithmic(true)
                .suffix(" m")
                .text("Brush size"),
        );
        ui.add(Slider::new(&mut self.strength, 0.05..=1.0).text("Strength"))
            .on_hover_text("Raise and Lower: up to half the brush size in meters per stroke");
        let mut changed = None;
        if self.tool == Some(TerrainTool::Paint) {
            changed = self.layers(ui, terrain.value.as_ref()?);
        }
        if self.tool.is_some() {
            ui.label(RichText::new("Paint to sculpt; hold Shift to invert.").color(MUTED));
        }
        changed
    }

    /// The surface layers to pick from and recolor, and a button that adds
    /// one; the component's new value if a layer changed.
    fn layers(&mut self, ui: &mut Ui, terrain: &Value) -> Option<Value> {
        let layers = terrain["layers"].as_array().cloned().unwrap_or_default();
        self.layer = self.layer.min(layers.len().saturating_sub(1));
        ui.label("Layers");
        let mut changed = None;
        for (index, layer) in layers.iter().enumerate() {
            ui.horizontal(|ui| {
                match srgb(layer) {
                    Some(mut color) => {
                        if ui.color_edit_button_srgb(&mut color).changed() {
                            let mut layers = layers.clone();
                            layers[index] = srgba(color);
                            changed = Some(with_layers(terrain, layers));
                        }
                    }
                    None => {
                        ui.label(RichText::new("?").color(MUTED))
                            .on_hover_text("Set as another color space in the file");
                    }
                }
                let label = format!("Layer {}", index + 1);
                if ui
                    .add(egui::Button::selectable(self.layer == index, label))
                    .clicked()
                {
                    self.layer = index;
                }
            });
        }
        let full = layers.len() >= MAX_LAYERS;
        let add = ui
            .add_enabled(!full, egui::Button::new("➕ Add layer"))
            .on_disabled_hover_text("A terrain has at most 4 layers.");
        if add.clicked() {
            let mut layers = layers.clone();
            layers.push(srgba([255, 255, 255]));
            changed = Some(with_layers(terrain, layers));
        }
        changed
    }
}

/// A layer's color as the game reports an sRGB `Color`, in bytes.
fn srgb(color: &Value) -> Option<[u8; 3]> {
    let color = color.get("Srgba")?;
    let channel = |name: &str| {
        let value = color[name].as_f64()?;
        Some((value.clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    Some([channel("red")?, channel("green")?, channel("blue")?])
}

fn srgba(color: [u8; 3]) -> Value {
    let [red, green, blue] = color.map(|byte| byte as f64 / 255.0);
    json!({"Srgba": {"red": red, "green": green, "blue": blue, "alpha": 1.0}})
}

fn with_layers(terrain: &Value, layers: Vec<Value>) -> Value {
    let mut terrain = terrain.clone();
    terrain["layers"] = Value::Array(layers);
    terrain
}

#[cfg(test)]
mod tests {
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    use super::*;

    fn terrain(layers: usize) -> ComponentData {
        let layer = srgba([90, 160, 70]);
        ComponentData {
            name: "Terrain".into(),
            path: TERRAIN.into(),
            mutable: true,
            required_by: None,
            saved: true,
            value: Some(json!({"size": 200.0, "layers": vec![layer; layers]})),
        }
    }

    /// The tools for `component`, and what they changed.
    fn panel(
        component: ComponentData,
        can_sculpt: bool,
    ) -> Harness<'static, (TerrainTools, Option<Value>)> {
        Harness::builder().with_step_dt(1.0 / 60.0).build_ui_state(
            move |ui, (tools, changed): &mut (TerrainTools, Option<Value>)| {
                if let Some(value) = tools.ui(ui, &component, can_sculpt) {
                    *changed = Some(value);
                }
            },
            (TerrainTools::default(), None),
        )
    }

    #[test]
    fn picks_a_brush_and_turns_raise_into_lower_with_shift() {
        let mut harness = panel(terrain(4), true);
        harness.run();
        assert!(
            harness.state().0.brush(false).is_none(),
            "selects until a brush is picked"
        );
        harness.get_by_label("Raise").click();
        harness.run();
        let raise = harness.state().0.brush(false).unwrap();
        assert_eq!(raise.tool, TerrainTool::Raise);
        assert_eq!(
            raise.strength,
            0.5 * 5.0 / 2.0,
            "meters: half the size at full strength"
        );
        assert_eq!(
            harness.state().0.brush(true).unwrap().tool,
            TerrainTool::Lower
        );
        harness.get_by_label("Paint to sculpt; hold Shift to invert.");

        harness.get_by_label("Raise").click();
        harness.run();
        assert!(
            harness.state().0.brush(false).is_none(),
            "picked again, put away"
        );
    }

    #[test]
    fn paints_a_layer_and_adds_layers_up_to_four() {
        let mut harness = panel(terrain(4), true);
        harness.run();
        harness.get_by_label("Paint").click();
        harness.run();
        harness.get_by_label("Layer 3").click();
        harness.run();
        let paint = harness.state().0.brush(true).unwrap();
        assert_eq!((paint.tool, paint.layer), (TerrainTool::Paint, Some(2)));
        assert!(
            harness
                .get_by_label("➕ Add layer")
                .accesskit_node()
                .is_disabled(),
            "four at most"
        );

        let mut harness = panel(terrain(2), true);
        harness.run();
        harness.get_by_label("Paint").click();
        harness.run();
        harness.get_by_label("➕ Add layer").click();
        harness.run();
        let value = harness.state().1.clone().expect("a layer was added");
        assert_eq!(value["layers"].as_array().unwrap().len(), 3);
        assert_eq!(value["size"], 200.0, "the rest of the value stays");
    }

    #[test]
    fn says_to_update_an_old_plugin() {
        let mut harness = panel(terrain(4), false);
        harness.run();
        harness.get_by_label("Update besfa_editor_plugin in the game to sculpt terrain.");
        assert!(harness.query_by_label("Raise").is_none());
    }

    #[test]
    fn reads_and_writes_srgb_colors() {
        assert_eq!(srgb(&srgba([10, 200, 255])), Some([10, 200, 255]));
        assert_eq!(srgb(&json!({"LinearRgba": {"red": 1.0}})), None);
    }
}
