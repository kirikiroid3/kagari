//! User shader pipelines: one per (shader asset, role), built on first use from
//! `kagari_shader::compose(role, source)` after `kagari_shader::validate`. A shader that cannot be
//! built is warned about once and stays unavailable; the planner then draws its fallback.

use std::collections::{HashMap, HashSet};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use kagari_bytecode::AssetId;
use kagari_engine::AssetStore;
use kagari_shader::{COMPUTE_ENTRY, FRAGMENT_ENTRY, ShaderRole, VERTEX_ENTRY};

use crate::Gfx;
use crate::pipelines::{LINEAR_FORMAT, PREMUL, PipeDesc, pipeline};

pub(crate) enum UserPipeline {
    Raster(wgpu::RenderPipeline),
    Compute(wgpu::ComputePipeline),
}

/// The built user pipelines (`None` = cannot be built).
#[derive(Default)]
pub(crate) struct ShaderCache {
    pipelines: HashMap<(AssetId, ShaderRole), Option<UserPipeline>>,
    /// Shaders whose source is not loaded, warned about once (not cached, so a later load works).
    warned: HashSet<(AssetId, ShaderRole)>,
}

/// The result of `future` if it is ready at once (wgpu's native error scopes are).
fn ready<F: Future>(future: F) -> Option<F::Output> {
    let mut cx = Context::from_waker(Waker::noop());
    match pin!(future).poll(&mut cx) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

impl ShaderCache {
    /// Builds the pipeline of `id` under `role` on first use; whether it can be drawn.
    pub fn prepare(
        &mut self,
        gfx: &Gfx,
        assets: &AssetStore,
        id: AssetId,
        role: ShaderRole,
    ) -> bool {
        if let Some(p) = self.pipelines.get(&(id, role)) {
            return p.is_some();
        }
        let Some(user) = &gfx.pipes.user else {
            return false;
        };
        if role.is_compute() && user.motion.is_none() {
            return false;
        }
        let path = assets.path(id);
        let Some(source) = assets.shader_source(id) else {
            if self.warned.insert((id, role)) {
                log::warn!("shader `{path}` ({role}) is not loaded; not drawn");
            }
            return false;
        };
        if let Err(e) = kagari_shader::validate(role, source) {
            log::warn!("shader `{path}` ({role}): {e}");
            self.pipelines.insert((id, role), None);
            return false;
        }
        let device = &gfx.device;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let label = format!("kagari shader {path} ({role})");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&label),
            source: wgpu::ShaderSource::Wgsl(kagari_shader::compose(role, source).into()),
        });
        let built = match (role, &user.motion) {
            (ShaderRole::LocalMotion | ShaderRole::TransitionalMotion, Some(motion)) => {
                UserPipeline::Compute(device.create_compute_pipeline(
                    &wgpu::ComputePipelineDescriptor {
                        label: Some(&label),
                        layout: Some(&motion.layout),
                        module: &module,
                        entry_point: Some(COMPUTE_ENTRY),
                        compilation_options: Default::default(),
                        cache: None,
                    },
                ))
            }
            _ => {
                let (layout, blend) = match role {
                    ShaderRole::Color => (&user.color, Some(PREMUL)),
                    ShaderRole::Texture => (&user.texture, Some(PREMUL)),
                    ShaderRole::Mix => (&user.mix, None),
                    _ => (&user.mask, None),
                };
                UserPipeline::Raster(pipeline(
                    device,
                    PipeDesc {
                        label: &label,
                        layout,
                        module: &module,
                        vertex: VERTEX_ENTRY,
                        fragment: FRAGMENT_ENTRY,
                        format: LINEAR_FORMAT,
                        blend,
                        quad: true,
                    },
                ))
            }
        };
        let error = ready(scope.pop()).flatten();
        let entry = match error {
            Some(e) => {
                log::warn!("shader `{path}` ({role}): {e}");
                None
            }
            None => Some(built),
        };
        let ok = entry.is_some();
        self.pipelines.insert((id, role), entry);
        ok
    }

    /// A raster pipeline prepared earlier.
    pub fn render(&self, id: AssetId, role: ShaderRole) -> Option<&wgpu::RenderPipeline> {
        match self.pipelines.get(&(id, role))? {
            Some(UserPipeline::Raster(p)) => Some(p),
            _ => None,
        }
    }

    /// A compute pipeline prepared earlier.
    pub fn compute(&self, id: AssetId, role: ShaderRole) -> Option<&wgpu::ComputePipeline> {
        match self.pipelines.get(&(id, role))? {
            Some(UserPipeline::Compute(p)) => Some(p),
            _ => None,
        }
    }
}
