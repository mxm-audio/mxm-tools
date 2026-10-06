//! mxm-listener-hud: drop a sound and see what the listener hears in it.

use mxm_listener_hud::Hud;

/// The player's backends (`apps/mxm-player/src/main.rs`): Direct3D 12, Vulkan and Metal, never
/// wgpu's own GL. Nothing here shares a process with a plugin editor, so the player's reason does not
/// bind this window — but one set of backends across the collection's apps is one set to verify
/// (the plan's H4 names this set for its bloom pass).
const BACKENDS: eframe::wgpu::Backends = eframe::wgpu::Backends::DX12
    .union(eframe::wgpu::Backends::VULKAN)
    .union(eframe::wgpu::Backends::METAL);

struct App(Hud);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

fn main() -> eframe::Result<()> {
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = BACKENDS;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_clamp_size_to_monitor_size(true)
            .with_drag_and_drop(true)
            .with_title("mxm-listener-hud"),
        wgpu_options,
        ..Default::default()
    };
    let result = eframe::run_native(
        "mxm-listener-hud",
        options,
        Box::new(|_cc| Ok(Box::new(App(Hud::new())))),
    );
    if let Err(error) = &result {
        eprintln!(
            "mxm-listener-hud could not start: {error}\n\
             It needs a Direct3D 12, Vulkan or Metal capable graphics adapter."
        );
    }
    result
}
