//! Recording and submitting work: the 1.x `dispatch_sequence` (copies, a
//! chain of dispatches, the transition for presentation, timestamps) as a
//! builder.

use std::cell::Cell;
use std::marker::PhantomData;

use crate::{Buffer, ComputePipeline, ExternalSemaphore, Result, Texture};

/// One dispatch: pipeline, workgroup counts, push constants.
#[derive(Clone, Copy)]
pub struct Dispatch<'a> {
    /// Pipeline.
    pub pipeline: &'a ComputePipeline,
    /// Workgroup counts.
    pub groups: [u32; 3],
    /// Push constant bytes (at most the pipeline's push size).
    pub push: &'a [u8],
    /// Name of the timestamp of this dispatch (stats panel: «ms por etapa»);
    /// `None` = no timestamp.
    pub timestamp: Option<&'a str>,
}

/// A rectangle copied between a buffer and a texture, or two textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CopyRegion {
    /// Source byte offset (buffer → texture) or source x.
    pub src_offset: u64,
    /// Source row length in texels (buffer → texture; 0 = tightly packed).
    pub src_row_length: u32,
    /// Source y (texture → texture).
    pub src_y: u32,
    /// Destination x.
    pub dst_x: u32,
    /// Destination y.
    pub dst_y: u32,
    /// Destination array layer.
    pub dst_slice: u32,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
}

/// A command buffer being recorded. `Send`, not `Sync`. Barriers between
/// steps are inserted by the recorder (resources are tracked per recording,
/// as the 1.x C++ does).
pub struct CommandRecorder {
    _private: (),
    _not_sync: PhantomData<Cell<()>>,
}

impl CommandRecorder {
    /// Copy regions of a buffer (capture staging) into a texture.
    pub fn copy_buffer_to_texture(&mut self, src: &Buffer, dst: &Texture, regions: &[CopyRegion]) {
        todo!("front G")
    }

    /// Copy regions of one texture into another (tiles, lsfg inputs).
    pub fn copy_texture(&mut self, src: &Texture, dst: &Texture, regions: &[CopyRegion]) {
        todo!("front G")
    }

    /// Copy a texture into a buffer (screenshots, readback).
    pub fn copy_texture_to_buffer(&mut self, src: &Texture, dst: &Buffer) {
        todo!("front G")
    }

    /// Fill a texture with one color.
    pub fn clear(&mut self, texture: &Texture, rgba: [f32; 4]) {
        todo!("front G")
    }

    /// Record a dispatch.
    pub fn dispatch(&mut self, dispatch: &Dispatch<'_>) {
        todo!("front G")
    }

    /// Leave `texture` ready to be copied to a swapchain image.
    pub fn prepare_present(&mut self, texture: &Texture) {
        todo!("front G")
    }

    /// Number of steps recorded.
    pub fn len(&self) -> usize {
        todo!("front G")
    }

    /// Nothing recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How to submit.
#[derive(Default)]
pub struct SubmitOptions<'a> {
    /// Read the dispatch timestamps back ([`Submission::wait`] returns them).
    pub timestamps: bool,
    /// Wait on these (lsfg output) before the work starts.
    pub wait: &'a [&'a ExternalSemaphore],
    /// Signal these (lsfg input ready) when the work ends.
    pub signal: &'a [&'a ExternalSemaphore],
}

/// GPU times of one submission.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timestamps {
    /// `(name, ms)` of every dispatch that asked for a timestamp, in order.
    pub dispatches: Vec<(String, f64)>,
    /// From the first to the last timestamp, in ms.
    pub total_ms: f64,
}

/// Work in flight.
pub struct Submission {
    _private: (),
}

impl Submission {
    /// Whether it finished (does not block).
    pub fn is_done(&self) -> Result<bool> {
        todo!("front G")
    }

    /// Block until it finishes (`frame_timeout`, in ns) and return the
    /// timestamps if they were asked for.
    pub fn wait(self, timeout_ns: u64) -> Result<Option<Timestamps>> {
        todo!("front G")
    }
}
