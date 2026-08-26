//! SCP output (display) management.

use crate::scp::protocol::{OutputId, OutputMode, Rect, SubpixelLayout, Transform};

/// Output (display) information.
#[derive(Debug, Clone)]
pub struct Output {
    pub id: OutputId,
    pub name: String,
    pub description: String,
    pub geometry: Rect,
    pub physical_size: (i32, i32), // mm
    pub subpixel: SubpixelLayout,
    pub transform: Transform,
    pub scale: i32,
    pub modes: Vec<OutputMode>,
    pub current_mode: usize,
}

impl Output {
    pub fn new(
        id: OutputId,
        name: String,
        description: String,
        width: i32,
        height: i32,
        refresh_rate: i32,
    ) -> Self {
        let mode = OutputMode {
            width,
            height,
            refresh_rate,
            preferred: true,
        };
        Self {
            id,
            name,
            description,
            geometry: Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            physical_size: (0, 0), // Unknown
            subpixel: SubpixelLayout::Unknown,
            transform: Transform::Normal,
            scale: 1,
            modes: vec![mode],
            current_mode: 0,
        }
    }

    pub fn current_mode(&self) -> &OutputMode {
        &self.modes[self.current_mode]
    }

    pub fn set_mode(&mut self, mode_index: usize) -> bool {
        if mode_index < self.modes.len() {
            self.current_mode = mode_index;
            let mode = &self.modes[mode_index];
            self.geometry.width = mode.width;
            self.geometry.height = mode.height;
            true
        } else {
            false
        }
    }

    pub fn set_scale(&mut self, scale: i32) {
        self.scale = scale;
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
        // Swap width/height for 90/270 rotations
        if matches!(
            transform,
            Transform::Rotate90
                | Transform::Rotate270
                | Transform::Flipped90
                | Transform::Flipped270
        ) {
            std::mem::swap(&mut self.geometry.width, &mut self.geometry.height);
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.geometry.x = x;
        self.geometry.y = y;
    }
}

/// Output manager — tracks all outputs and their configuration.
#[derive(Debug, Default)]
pub struct OutputManager {
    outputs: Vec<Output>,
    next_id: OutputId,
}

impl OutputManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_output(
        &mut self,
        name: String,
        description: String,
        width: i32,
        height: i32,
        refresh_rate: i32,
    ) -> OutputId {
        let id = self.next_id;
        self.next_id += 1;
        let output = Output::new(id, name, description, width, height, refresh_rate);
        self.outputs.push(output);
        id
    }

    pub fn remove_output(&mut self, id: OutputId) -> Option<Output> {
        if let Some(pos) = self.outputs.iter().position(|o| o.id == id) {
            Some(self.outputs.remove(pos))
        } else {
            None
        }
    }

    pub fn get_output(&self, id: OutputId) -> Option<&Output> {
        self.outputs.iter().find(|o| o.id == id)
    }

    pub fn get_output_mut(&mut self, id: OutputId) -> Option<&mut Output> {
        self.outputs.iter_mut().find(|o| o.id == id)
    }

    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    pub fn outputs_mut(&mut self) -> &mut [Output] {
        &mut self.outputs
    }

    pub fn primary_output(&self) -> Option<&Output> {
        self.outputs.first()
    }
}
