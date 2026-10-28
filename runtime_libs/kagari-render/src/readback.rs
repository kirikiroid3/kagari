//! `render_to_rgba`: the design target resolved into an `Rgba8UnormSrgb` texture, copied to a
//! mappable buffer and read back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::Gfx;
use crate::pipelines::READBACK_FORMAT;

/// The resolve texture and the buffer it is copied into, for one size.
pub(crate) struct Readback {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    pub size: (u32, u32),
    /// Bytes per buffer row: `width·4` rounded up to 256.
    padded: u32,
}

impl Readback {
    pub fn new(gfx: &Gfx, size: (u32, u32)) -> Option<Readback> {
        let row = size.0.checked_mul(4)?;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = row.div_ceil(align).checked_mul(align)?;
        let texture = gfx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kagari readback"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: READBACK_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let buffer = gfx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kagari readback"),
            size: u64::from(padded).checked_mul(u64::from(size.1))?,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Readback {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            buffer,
            size,
            padded,
        })
    }

    /// Records the texture → buffer copy.
    pub fn copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded),
                    rows_per_image: Some(self.size.1),
                },
            },
            wgpu::Extent3d {
                width: self.size.0,
                height: self.size.1,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Maps the buffer after `submission` and returns the rows without padding.
    pub fn read(&self, gfx: &Gfx, submission: wgpu::SubmissionIndex) -> Result<Vec<u8>, String> {
        let result: Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>> = Arc::new(Mutex::new(None));
        let sink = result.clone();
        self.buffer.map_async(wgpu::MapMode::Read, .., move |r| {
            if let Ok(mut slot) = sink.lock() {
                *slot = Some(r);
            }
        });
        gfx.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })
            .map_err(|e| e.to_string())?;
        let mapped = result
            .lock()
            .map_err(|_| "the map callback panicked".to_owned())?
            .take();
        match mapped {
            Some(Ok(())) => {}
            Some(Err(e)) => return Err(e.to_string()),
            None => return Err("the buffer was not mapped in time".to_owned()),
        }
        let out = {
            let view = self.buffer.get_mapped_range(..).map_err(|e| e.to_string())?;
            let row = (self.size.0 as usize).saturating_mul(4);
            let mut out = Vec::with_capacity(row.saturating_mul(self.size.1 as usize));
            for chunk in view.chunks(self.padded as usize).take(self.size.1 as usize) {
                out.extend_from_slice(chunk.get(..row).unwrap_or(chunk));
            }
            out
        };
        self.buffer.unmap();
        Ok(out)
    }
}
