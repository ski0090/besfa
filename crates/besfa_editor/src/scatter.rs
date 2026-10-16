//! The Scatter tab beside the Inspector: the prefabs and models to
//! scatter and how often each comes up, and the brush's settings. With
//! Scatter on in the toolbar, the left button places them in the scene
//! view, and erases them with Shift.

use besfa_protocol::{ScatterBrush, ScatterModel};
use eframe::egui::{self, DragValue, RichText, Slider, Ui};

use crate::panels::{MUTED, can_place, section};

pub struct ScatterTools {
    /// Painting with the left button instead of selecting.
    pub on: bool,
    /// The models and prefabs, and how often each comes up.
    pub models: Vec<ScatterModel>,
    /// The brush's radius in meters.
    pub size: f32,
    /// Things per 100 square meters.
    pub density: f32,
    pub spacing: f32,
    pub scale: [f32; 2],
    pub rotate: bool,
    pub align: bool,
}

impl Default for ScatterTools {
    fn default() -> Self {
        Self {
            on: false,
            models: Vec::new(),
            size: 5.0,
            density: 10.0,
            spacing: 1.0,
            scale: [0.8, 1.2],
            rotate: true,
            align: false,
        }
    }
}

impl ScatterTools {
    /// The brush to scatter with, while on and with something to place;
    /// Shift erases instead.
    pub fn brush(&self, shift: bool) -> Option<ScatterBrush> {
        if !self.on || self.models.is_empty() {
            return None;
        }
        Some(ScatterBrush {
            models: self.models.clone(),
            radius: self.size,
            density: self.density,
            spacing: self.spacing,
            scale: self.scale,
            rotate: self.rotate,
            align: self.align,
            erase: shift,
        })
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        section(ui, "Scatter");
        let (_, dropped) = ui.dnd_drop_zone::<String, _>(egui::Frame::NONE, |ui| {
            ui.set_min_width(ui.available_width());
            if self.models.is_empty() {
                ui.label(
                    RichText::new("Drag prefabs or models here to scatter them.").color(MUTED),
                );
            }
            let mut gone = None;
            for (index, model) in self.models.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("✕").on_hover_text("Remove").clicked() {
                        gone = Some(index);
                    }
                    ui.add(
                        DragValue::new(&mut model.weight)
                            .range(0.0..=10.0)
                            .speed(0.05),
                    )
                    .on_hover_text("How often it comes up against the others");
                    let name = model.path.rsplit('/').next().unwrap_or(&model.path);
                    ui.label(name).on_hover_text(model.path.as_str());
                });
            }
            if let Some(index) = gone {
                self.models.remove(index);
            }
        });
        if let Some(path) = dropped
            && can_place(&path)
            && !self.models.iter().any(|model| model.path == *path)
        {
            self.models.push(ScatterModel {
                path: (*path).clone(),
                weight: 1.0,
            });
        }
        ui.add_space(8.0);
        ui.add(
            Slider::new(&mut self.size, 0.5..=50.0)
                .logarithmic(true)
                .suffix(" m")
                .text("Brush size"),
        );
        ui.add(
            Slider::new(&mut self.density, 1.0..=200.0)
                .logarithmic(true)
                .text("Density"),
        )
        .on_hover_text("Things per 100 m² under the brush");
        ui.add(
            Slider::new(&mut self.spacing, 0.0..=20.0)
                .suffix(" m")
                .text("Spacing"),
        );
        ui.horizontal(|ui| {
            let [small, big] = &mut self.scale;
            ui.add(DragValue::new(small).range(0.05..=*big).speed(0.01));
            ui.label("to");
            ui.add(DragValue::new(big).range(*small..=20.0).speed(0.01));
            ui.label("Scale");
        });
        ui.checkbox(&mut self.rotate, "Random rotation");
        ui.checkbox(&mut self.align, "Align to surface");
        if self.on {
            ui.label(RichText::new("Paint to place; hold Shift to erase.").color(MUTED));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;

    #[test]
    fn scatters_only_while_on_with_something_listed_and_erases_with_shift() {
        let mut tools = ScatterTools::default();
        assert!(tools.brush(false).is_none());
        tools.on = true;
        assert!(tools.brush(false).is_none(), "nothing to place");
        tools.models.push(ScatterModel {
            path: "models/oak.glb".into(),
            weight: 2.0,
        });
        let brush = tools.brush(false).unwrap();
        assert_eq!(brush.models[0].weight, 2.0);
        assert!(!brush.erase);
        assert!(tools.brush(true).unwrap().erase);
        assert!(can_place("prefabs/rock.scn.ron") && !can_place("textures/grass.png"));
    }

    #[test]
    fn lists_what_to_scatter_or_says_how_to_add_it() {
        let mut harness = Harness::new_ui_state(
            |ui, tools: &mut ScatterTools| tools.ui(ui),
            ScatterTools::default(),
        );
        harness.run();
        harness.get_by_label("Drag prefabs or models here to scatter them.");

        harness.state_mut().models.push(ScatterModel {
            path: "models/oak.glb".into(),
            weight: 1.0,
        });
        harness.state_mut().on = true;
        harness.run();
        harness.get_by_label("oak.glb");
        harness.get_by_label("Paint to place; hold Shift to erase.");
        harness.get_by_label("✕").click();
        harness.run();
        assert!(harness.state().models.is_empty(), "removed");
    }
}
