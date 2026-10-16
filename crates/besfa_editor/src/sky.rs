//! The Sky tab: the scene's sky, a gradient from the ground up to the
//! zenith or a cubemap image. It is one component the tab sets as a whole,
//! so undo, saving and `besfa_mcp`'s `set` treat it like any other, and a
//! game's code moving the sky in play shows here as it goes.

use eframe::egui::{self, Id, RichText, Slider, Ui};
use serde_json::Value;

use crate::{
    inspector::{Action, commit_field},
    panels::{MUTED, section, tag},
    scene::Scene,
    terrain::{srgb, srgba},
};

pub const SKY: &str = "besfa_editor_plugin::sky::Sky";

/// What the tab asks of the editor.
#[derive(Debug, PartialEq)]
pub enum SkyAction {
    /// Select the sky's entity, so the game reports its value.
    Select(u64),
    /// Spawn a sky.
    Add,
    Edit(Action),
}

/// The tab; `known` is false for a plugin too old for the sky, and
/// `editing` false in play, which adds nothing to the scene.
pub fn ui(ui: &mut Ui, scene: &Scene, known: bool, editing: bool) -> Option<SkyAction> {
    if !known {
        tag(
            ui,
            "Update besfa_editor_plugin in the game to edit the sky.",
        );
        return None;
    }
    let Some(sky) = scene.entities.iter().find(|entity| entity.sky) else {
        tag(ui, "The scene has no sky; the camera's clear color shows.");
        return ui
            .add_enabled(editing, egui::Button::new("➕ Add sky"))
            .clicked()
            .then_some(SkyAction::Add);
    };
    let value = scene
        .components
        .iter()
        .find(|component| component.path == SKY && scene.selected == Some(sky.id))
        .and_then(|component| component.value.as_ref());
    let Some(value) = value else {
        tag(ui, "Select the sky to edit it.");
        return ui
            .button(format!("Edit {}", sky.label()))
            .clicked()
            .then_some(SkyAction::Select(sky.id));
    };
    let mut edited = value.clone();
    edit(ui, Id::new(("sky", sky.id)), &mut edited);
    if !editing {
        tag(ui, "Changes made in play are gone after Stop.");
    }
    (edited != *value).then(|| SkyAction::Edit(Action::Set(SKY.into(), edited)))
}

/// The sky's fields, edited in place.
fn edit(ui: &mut Ui, id: Id, sky: &mut Value) {
    section(ui, "Sky");
    let cubemap = sky["mode"] == "Cubemap";
    ui.horizontal(|ui| {
        for (mode, label) in [("Gradient", "Gradient"), ("Cubemap", "Cubemap")] {
            if ui
                .add(egui::Button::selectable(sky["mode"] == mode, label))
                .clicked()
            {
                sky["mode"] = mode.into();
            }
        }
    });
    if cubemap {
        image(ui, id.with("cubemap"), sky);
        tag(
            ui,
            "The gradient shows while the image loads, or if it cannot.",
        );
    } else {
        for (field, label) in [
            ("zenith", "Zenith"),
            ("horizon", "Horizon"),
            ("ground", "Ground"),
        ] {
            color(ui, &mut sky[field], label);
        }
        number(ui, sky, "blend", |number| {
            Slider::new(number, 0.05..=4.0)
                .logarithmic(true)
                .text("Blend")
        })
        .on_hover_text("How far the horizon's color reaches up; less shows more of the zenith");
        // Plugins from before the sun and moon have neither.
        for (field, label) in [("sun", "Sun"), ("moon", "Moon")] {
            if let Some(body) = sky.get_mut(field) {
                section(ui, label);
                disc(ui, body, field == "moon");
            }
        }
    }
    section(ui, "Both");
    number(ui, sky, "brightness", |number| {
        Slider::new(number, 0.0..=4.0).text("Brightness")
    })
    .on_hover_text("1 shows the colors as picked");
    number(ui, sky, "rotation", |number| {
        Slider::new(number, -180.0..=180.0)
            .suffix("°")
            .text("Rotation")
    })
    .on_hover_text("Turns the sky counterclockwise, seen from above");
}

/// A color button and its label; `?` for a color it cannot show.
fn color(ui: &mut Ui, value: &mut Value, label: &str) {
    ui.horizontal(|ui| {
        match srgb(value) {
            Some(mut color) => {
                if ui.color_edit_button_srgb(&mut color).changed() {
                    *value = srgba(color);
                }
            }
            None => {
                ui.label(RichText::new("?").color(MUTED))
                    .on_hover_text("Set as another color space");
            }
        }
        ui.label(label);
    });
}

/// The sun's or the moon's fields; the moon has a phase.
fn disc(ui: &mut Ui, body: &mut Value, moon: bool) {
    let mut show = body["show"].as_bool().unwrap_or_default();
    if ui.checkbox(&mut show, "Show").changed() {
        body["show"] = show.into();
    }
    color(ui, &mut body["color"], "Color");
    number(ui, body, "size", |number| {
        Slider::new(number, 0.5..=30.0)
            .logarithmic(true)
            .suffix("°")
            .text("Size")
    })
    .on_hover_text("Degrees across");
    let [mut elevation, mut azimuth] = angles(&body["direction"]);
    let raised = ui
        .add(
            Slider::new(&mut elevation, -90.0..=90.0)
                .suffix("°")
                .text("Elevation"),
        )
        .on_hover_text("Above the horizon");
    let turned = ui
        .add(
            Slider::new(&mut azimuth, 0.0..=360.0)
                .suffix("°")
                .text("Azimuth"),
        )
        .on_hover_text("From north (-Z) toward east (+X)");
    if raised.changed() || turned.changed() {
        body["direction"] = direction(elevation, azimuth);
    }
    if moon {
        number(ui, body, "phase", |number| {
            Slider::new(number, 0.0..=1.0).text("Phase")
        })
        .on_hover_text("0 new, 0.25 first quarter, 0.5 full, 0.75 last quarter");
        let phase = body["phase"].as_f64().unwrap_or_default() as f32;
        tag(ui, phase_name(phase));
    }
}

/// A direction `[x, y, z]` as its elevation and azimuth in degrees.
fn angles(direction: &Value) -> [f32; 2] {
    let at = |index: usize| direction[index].as_f64().unwrap_or_default() as f32;
    let [x, y, z] = [at(0), at(1), at(2)];
    let length = (x * x + y * y + z * z).sqrt();
    if length == 0.0 {
        return [90.0, 0.0];
    }
    let elevation = (y / length).clamp(-1.0, 1.0).asin().to_degrees();
    let azimuth = x.atan2(-z).to_degrees().rem_euclid(360.0);
    [elevation, azimuth]
}

/// The unit direction `[x, y, z]` at `elevation` and `azimuth` in degrees.
fn direction(elevation: f32, azimuth: f32) -> Value {
    let (elevation, azimuth) = (elevation.to_radians(), azimuth.to_radians());
    let level = elevation.cos();
    let round = |value: f32| ((value * 1e4).round() / 1e4) as f64;
    serde_json::json!([
        round(azimuth.sin() * level),
        round(elevation.sin()),
        round(-azimuth.cos() * level)
    ])
}

/// What the moon looks like at `phase`.
fn phase_name(phase: f32) -> &'static str {
    const NAMES: [&str; 8] = [
        "New",
        "Waxing crescent",
        "First quarter",
        "Waxing gibbous",
        "Full",
        "Waning gibbous",
        "Last quarter",
        "Waning crescent",
    ];
    NAMES[((phase.rem_euclid(1.0) * 8.0).round() as usize) % 8]
}

/// The `slider` made for the number `sky[field]`.
fn number(
    ui: &mut Ui,
    sky: &mut Value,
    field: &str,
    slider: impl FnOnce(&mut f32) -> Slider<'_>,
) -> egui::Response {
    let mut number = sky[field].as_f64().unwrap_or_default() as f32;
    let response = ui.add(slider(&mut number));
    if response.changed() {
        sky[field] = (number as f64).into();
    }
    response
}

/// The cubemap's path, typed or dragged from the Assets tab.
fn image(ui: &mut Ui, id: Id, sky: &mut Value) {
    let path = sky["cubemap"].as_str().unwrap_or_default().to_owned();
    let (_, dropped) = ui.dnd_drop_zone::<String, _>(egui::Frame::NONE, |ui| {
        ui.set_min_width(ui.available_width());
        if let Some(typed) = commit_field(ui, id, &path, Some("Image ")) {
            sky["cubemap"] = typed.trim().into();
        }
    });
    if let Some(dropped) = dropped.filter(|path| is_image(path)) {
        sky["cubemap"] = (*dropped).clone().into();
    }
    tag(
        ui,
        "A .ktx2 or .dds cubemap, or six faces (+X, -X, +Y, -Y, +Z, -Z) stacked top to bottom.",
    );
}

/// Files that can be a cubemap.
fn is_image(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".ktx2", ".dds"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use besfa_protocol::{ComponentData, EntityInfo};
    use egui_kittest::{Harness, kittest::Queryable};
    use serde_json::json;

    use super::*;

    fn sky_entity() -> EntityInfo {
        EntityInfo {
            id: 3,
            name: Some("Sky".into()),
            parent: None,
            scene: true,
            asset: None,
            sky: true,
        }
    }

    fn sky_value() -> Value {
        json!({
            "mode": "Gradient",
            "zenith": srgba([50, 100, 200]),
            "horizon": srgba([180, 210, 240]),
            "ground": {"LinearRgba": {"red": 0.1, "green": 0.1, "blue": 0.1, "alpha": 1.0}},
            "blend": 0.5,
            "cubemap": "",
            "brightness": 1.0,
            "rotation": 0.0,
            "sun": {"show": true, "direction": [0.0, 1.0, -1.0], "size": 3.0, "color": srgba([255, 245, 220])},
            "moon": {"show": false, "direction": [0.0, 0.5, 1.0], "size": 4.0, "color": srgba([225, 230, 240]), "phase": 0.25},
        })
    }

    fn selected_sky() -> Scene {
        let mut scene = Scene::default();
        scene.entities = vec![sky_entity()];
        scene.selected = Some(3);
        scene.components = vec![ComponentData {
            name: "Sky".into(),
            path: SKY.into(),
            mutable: true,
            required_by: None,
            saved: true,
            value: Some(sky_value()),
        }];
        scene
    }

    /// The tab for `scene`, and what it asked last.
    fn tab(scene: Scene, editing: bool) -> Harness<'static, (Scene, Option<SkyAction>)> {
        Harness::new_ui_state(
            move |ui, (scene, asked): &mut (Scene, Option<SkyAction>)| {
                if let Some(action) = super::ui(ui, scene, true, editing) {
                    *asked = Some(action);
                }
            },
            (scene, None),
        )
    }

    #[test]
    fn offers_to_add_a_sky_or_select_the_one_there_is() {
        let mut harness = tab(Scene::default(), true);
        harness.run();
        harness.get_by_label("➕ Add sky").click();
        harness.run();
        assert_eq!(harness.state().1, Some(SkyAction::Add));

        let mut scene = Scene::default();
        scene.entities = vec![sky_entity()];
        let mut harness = tab(scene, true);
        harness.run();
        harness.get_by_label("Edit Sky").click();
        harness.run();
        assert_eq!(harness.state().1, Some(SkyAction::Select(3)));
    }

    #[test]
    fn sets_the_whole_sky_when_the_mode_changes() {
        let mut harness = tab(selected_sky(), true);
        harness.run();
        harness.get_by_label("Zenith");
        harness.get_by_label("Cubemap").click();
        harness.run();
        let Some(SkyAction::Edit(Action::Set(path, value))) = harness.state().1.as_ref() else {
            panic!("asked {:?}", harness.state().1);
        };
        assert_eq!(path, SKY);
        assert_eq!(value["mode"], "Cubemap");
        assert_eq!(value["zenith"], sky_value()["zenith"], "the rest stays");
    }

    #[test]
    fn edits_the_sun_and_moon_and_names_the_phase() {
        let mut harness = tab(selected_sky(), true);
        harness.run();
        harness.get_by_label("Sun");
        harness.get_by_label("First quarter");
        harness.get_all_by_label("Show").next().unwrap().click();
        harness.run();
        let Some(SkyAction::Edit(Action::Set(_, value))) = harness.state().1.as_ref() else {
            panic!("asked {:?}", harness.state().1);
        };
        assert_eq!(value["sun"]["show"], false);
        assert_eq!(value["moon"], sky_value()["moon"], "the moon stays");

        assert_eq!(angles(&json!([0.0, 1.0, -1.0])), [45.0, 0.0]);
        let [elevation, azimuth] = angles(&direction(30.0, 90.0));
        assert!((elevation - 30.0).abs() < 0.01 && (azimuth - 90.0).abs() < 0.01);
        assert_eq!(direction(0.0, 90.0), json!([1.0, 0.0, 0.0]), "east is +X");
        assert_eq!(phase_name(0.5), "Full");
        assert_eq!(phase_name(0.98), "New");
    }

    #[test]
    fn says_play_changes_go_and_reads_linear_colors() {
        let mut harness = tab(selected_sky(), false);
        harness.run();
        harness.get_by_label("Changes made in play are gone after Stop.");
        // The ground is linear, as a game may write it; it still shows.
        assert!(harness.query_by_label("?").is_none());
        assert!(is_image("skies/Night.PNG") && !is_image("models/oak.glb"));
    }
}
