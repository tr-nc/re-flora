use super::App;
use crate::app::world_edits::{BuildEdit, ClearVoxelRegionEdit, VoxelEdit, WorldEditPlan};
use crate::app::world_ops;
use crate::builder::{ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder};
use crate::geom::UAabb3;
use crate::util::BENCH;
use crate::window::{select_monitor_by_score, WindowMode, WindowState, WindowStateDesc};
use anyhow::Result;
use glam::UVec3;
use verdarium_vkn::{VulkanContext, VulkanContextDesc};
use winit::event_loop::ActiveEventLoop;

impl App {
    #[allow(dead_code)]
    pub(super) fn init(
        plain_builder: &mut PlainBuilder,
        surface_builder: &mut SurfaceBuilder,
        contree_builder: &mut ContreeBuilder,
        scene_accel_builder: &mut SceneAccelBuilder,
    ) -> Result<()> {
        let world_dim = super::VOXEL_DIM_PER_CHUNK * super::CHUNK_DIM;
        let world_bound = UAabb3::new(UVec3::ZERO, world_dim - UVec3::ONE);
        world_ops::execute_edit_plan_on_builders(
            plain_builder,
            surface_builder,
            contree_builder,
            scene_accel_builder,
            super::VOXEL_DIM_PER_CHUNK,
            WorldEditPlan {
                voxel_edits: vec![
                    VoxelEdit::ClearVoxelRegion(ClearVoxelRegionEdit {
                        offset: UVec3::ZERO,
                        dim: world_dim,
                    }),
                    super::startup_water_pool_inverted_pyramid_voxel_edit()?,
                ],
                build_edits: vec![BuildEdit::RebuildMesh(world_bound)],
            },
        )?;

        BENCH.lock().unwrap().summary();
        Ok(())
    }

    pub(super) fn create_window_state(
        event_loop: &ActiveEventLoop,
        options: &crate::AppOptions,
    ) -> WindowState {
        const WINDOW_TITLE_DEBUG: &str = "Verdarium - debug build";
        const WINDOW_TITLE_RELEASE: &str = "Verdarium - release build";
        let using_mode = if cfg!(debug_assertions) {
            WINDOW_TITLE_DEBUG
        } else {
            WINDOW_TITLE_RELEASE
        };
        let fullscreen_monitor = if options.windowed {
            None
        } else {
            select_monitor_by_score(event_loop, options.monitor_score)
        };
        let hidden_fullscreen_monitor_size = if options.hidden && !options.windowed {
            fullscreen_monitor.as_ref().map(|monitor| {
                let size = monitor.size();
                let scale_factor = monitor.scale_factor() as f32;
                (
                    size.width as f32 / scale_factor,
                    size.height as f32 / scale_factor,
                )
            })
        } else {
            None
        };

        let window_mode = if options.hidden || options.windowed {
            WindowMode::Windowed(false)
        } else {
            WindowMode::BorderlessFullscreen
        };
        let mut window_descriptor = WindowStateDesc {
            title: using_mode.to_owned(),
            window_mode,
            fullscreen_monitor,
            cursor_locked: !options.hidden,
            cursor_visible: options.hidden,
            visible: !options.hidden,
            ..Default::default()
        };
        if let Some((width, height)) = hidden_fullscreen_monitor_size {
            window_descriptor.width = width;
            window_descriptor.height = height;
        } else if options.hidden && !options.windowed {
            log::warn!(
                "No scored monitor available for --hidden; falling back to default windowed extent"
            );
        }
        if options.hidden {
            log::info!(
                "Running with a hidden native window at {:.0}x{:.0} logical pixels; Vulkan surface/swapchain path is unchanged",
                window_descriptor.width,
                window_descriptor.height,
            );
        }
        let window_state = WindowState::new(event_loop, &window_descriptor);
        if options.hidden {
            let extent = window_state.window_extent();
            log::info!(
                "Hidden window render extent is {}x{} physical pixels",
                extent.width,
                extent.height,
            );
        }
        window_state
    }

    pub(super) fn create_vulkan_context(window_state: &WindowState) -> VulkanContext {
        VulkanContext::new(
            &window_state.window(),
            VulkanContextDesc {
                name: "Verdarium".into(),
            },
        )
    }
}
