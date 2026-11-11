//! The Besfa editor: a project hub, and a scene editor that runs the game
//! in its viewport.

// A console only while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod animation;
mod editor;
mod hub;
mod i18n;
mod inspector;
mod organization;
mod panels;
mod preview;
mod process;
mod project;
mod review;
mod scatter;
mod scene;
mod terrain;
mod viewport;


use eframe::{
    egui::{self, ViewportCommand},
    egui_wgpu, wgpu,
};

use crate::{
    editor::Editor,
    hub::Hub,
    process::{Prebuild, PrebuildPhase},
    project::RecentProjects,
};

fn main() -> eframe::Result {
    kill_children_with_editor();
    // The game opens the viewport texture on D3D12, so the editor's device
    // must be D3D12 too.
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::DX12;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Besfa Editor")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 640.0]),
        centered: true,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Besfa Editor",
        options,
        Box::new(|cc| {
            besfa_ui::style(&cc.egui_ctx);
            i18n::load();
            let recent = RecentProjects::standard();
            Ok(Box::new(App {
                render: cc.wgpu_render_state.clone(),
                // Bevy builds in the background while the hub shows.
                prebuild: Prebuild::start(&cc.egui_ctx),
                page: Page::Hub(Hub::new(&recent)),
                recent,
                title: "Besfa Editor".into(),
            }))
        }),
    )
}

struct App {
    render: Option<egui_wgpu::RenderState>,
    /// Shown on every page; it runs while the hub shows and stops for a
    /// project's build.
    prebuild: Prebuild,
    page: Page,
    recent: RecentProjects,
    title: String,
}

enum Page {
    Hub(Hub),
    Editor(Box<Editor>),
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.prebuild.poll();
        let next = match &mut self.page {
            Page::Hub(hub) => hub.ui(ui, &self.recent, &self.prebuild).map(|project| {
                // The project builds now; the prebuild resumes in the hub.
                self.prebuild.stop();
                let editor = Editor::new(project, self.render.clone(), &self.prebuild, ui.ctx());
                Page::Editor(Box::new(editor))
            }),
            Page::Editor(editor) => editor.ui(ui, &self.prebuild).then(|| {
                if self.prebuild.phase == PrebuildPhase::Stopped {
                    self.prebuild = Prebuild::start(ui.ctx());
                }
                Page::Hub(Hub::new(&self.recent))
            }),
        };
        // Dropping the editor stops its game.
        if let Some(next) = next {
            self.page = next;
        }
        let title = match &self.page {
            Page::Hub(_) => "Besfa Editor".into(),
            Page::Editor(editor) => editor.title(),
        };
        if title != self.title {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// Builds and games the editor starts inherit this job, and the kernel kills
/// them when the job's last handle closes: when the editor exits, however it
/// exits. Without it a crashed editor leaves cargo and a window-less game
/// running. The handle is left open on purpose. What must outlive the
/// editor, like a browser it opened, breaks away from the job.
fn kill_children_with_editor() {
    use windows::Win32::System::{JobObjects::*, Threading::GetCurrentProcess};
    // SAFETY: plain Win32 calls on a job this process owns.
    unsafe {
        let Ok(job) = CreateJobObjectW(None, None) else {
            return;
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        let _ = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of_val(&limits) as u32,
        );
        let _ = AssignProcessToJobObject(job, GetCurrentProcess());
    }
}
