use super::mesh_renderer::MeshRenderer;
use crate::mesh::{
    animation::{self, AnimationClip},
    skeleton::Skeleton,
};
use crate::rpc::types::AnimationResponse;
use std::time::Instant;

pub struct PoseState {
    pub clips: Vec<AnimationClip>,
    pub current_clip: usize,
    pub time: f32,
    pub playing: bool,
    pub speed: f32,
    pub skeleton: Skeleton,
    pub last_frame: Instant,
}
impl PoseState {
    pub fn new(skeleton: Skeleton, clips: Vec<AnimationClip>, playing: bool) -> Self {
        Self {
            skeleton,
            clips,
            playing,
            current_clip: 0,
            time: 0.0,
            speed: 1.0,
            last_frame: Instant::now(),
        }
    }
    pub fn list(&self) -> Vec<AnimationResponse> {
        self.clips
            .iter()
            .enumerate()
            .map(|(index, clip)| AnimationResponse {
                name: clip.name.clone(),
                index,
                duration: clip.duration,
                time: if index == self.current_clip {
                    self.time
                } else {
                    0.0
                },
            })
            .collect()
    }
    pub fn load_clips(&mut self, path: &std::path::Path) -> Result<Vec<AnimationResponse>, String> {
        let clips = crate::mesh::clips::load(path, &self.skeleton).map_err(|e| e.to_string())?;
        self.clips = clips;
        self.current_clip = 0;
        self.time = 0.0;
        self.playing = false;
        Ok(self.list())
    }
    pub fn info(&self) -> Result<AnimationResponse, String> {
        let clip = self
            .clips
            .get(self.current_clip)
            .ok_or("No animation clips loaded")?;
        Ok(AnimationResponse {
            name: clip.name.clone(),
            index: self.current_clip,
            duration: clip.duration,
            time: self.time,
        })
    }
    pub fn select(&mut self, name: &str) -> Result<AnimationResponse, String> {
        let index = name
            .parse::<usize>()
            .ok()
            .filter(|&i| i < self.clips.len())
            .or_else(|| {
                self.clips
                    .iter()
                    .position(|c| c.name.as_deref() == Some(name))
            })
            .ok_or_else(|| format!("Animation '{name}' not found"))?;
        self.current_clip = index;
        self.time = 0.0;
        self.playing = false;
        self.info()
    }
    pub fn set_time(&mut self, time: f32) -> Result<AnimationResponse, String> {
        let clip = self
            .clips
            .get(self.current_clip)
            .ok_or("No animation clips loaded")?;
        if !time.is_finite() || time < 0.0 || time > clip.duration {
            return Err(format!("Time must be within 0..{} seconds", clip.duration));
        }
        self.time = time;
        self.playing = false;
        self.info()
    }
    pub fn set_frame(&mut self, frame: u32, fps: f32) -> Result<AnimationResponse, String> {
        if !fps.is_finite() || fps <= 0.0 || fps > 1000.0 {
            return Err("FPS must be finite and within (0,1000]".into());
        }
        self.set_time(frame as f32 / fps)
    }
    pub fn update(&mut self, renderer: &MeshRenderer, queue: &wgpu::Queue) {
        let now = Instant::now();
        if let Some(clip) = self.clips.get(self.current_clip) {
            if self.playing {
                self.time += (now - self.last_frame).as_secs_f32() * self.speed;
                if clip.duration > 0.0 {
                    self.time %= clip.duration;
                }
            }
            let transforms = animation::evaluate_animation(clip, &self.skeleton, self.time);
            renderer.update_joint_palette(
                queue,
                &self.skeleton.compute_joint_matrices_with_pose(&transforms),
            );
        } else {
            renderer.update_joint_palette(queue, &self.skeleton.compute_joint_matrices());
        }
        self.last_frame = now;
    }
}
