//! Hand-rolled BVH (Biovision Hierarchy) motion capture file parser.
//!
//! BVH is a plain-text format with two sections:
//! - HIERARCHY: recursive joint tree with offsets and channel declarations
//! - MOTION: flat float table of per-frame channel values

use std::fmt;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Xposition,
    Yposition,
    Zposition,
    Xrotation,
    Yrotation,
    Zrotation,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BvhJoint {
    pub name: String,
    pub offset: [f32; 3],
    pub channels: Vec<ChannelType>,
    pub parent: Option<usize>, // index into joints array
    pub is_end_site: bool,
}

#[derive(Debug, Clone)]
pub struct BvhClip {
    pub joints: Vec<BvhJoint>,
    pub frame_time: f32,
    pub frame_count: usize,
    pub frames: Vec<Vec<f32>>, // frames[frame_idx] = flat channel values
}

impl BvhClip {
    /// Returns the offset into a frame's float array where `joint_idx`'s channels start.
    pub fn joint_channel_offset(&self, joint_idx: usize) -> usize {
        self.joints[..joint_idx]
            .iter()
            .map(|j| j.channels.len())
            .sum()
    }

    /// Total clip duration in seconds.
    pub fn duration(&self) -> f32 {
        if self.frame_count <= 1 {
            0.0
        } else {
            self.frame_time * (self.frame_count - 1) as f32
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct BvhError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for BvhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BVH parse error (line {}): {}", self.line, self.message)
    }
}

impl std::error::Error for BvhError {}

// ---------------------------------------------------------------------------
// Parser internals
// ---------------------------------------------------------------------------

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    joints: Vec<BvhJoint>,
    total_channels: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lines: input.lines().collect(),
            pos: 0,
            joints: Vec::new(),
            total_channels: 0,
        }
    }

    fn err(&self, msg: impl Into<String>) -> BvhError {
        BvhError {
            line: self.pos + 1,
            message: msg.into(),
        }
    }

    /// Return the current trimmed line and advance, skipping blank lines.
    fn next_line(&mut self) -> Result<&'a str, BvhError> {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            self.pos += 1;
            if !line.is_empty() {
                return Ok(line);
            }
        }
        Err(self.err("unexpected end of file"))
    }

    /// Peek at the current non-blank line without consuming it.
    #[allow(dead_code)]
    fn peek_line(&self) -> Option<&'a str> {
        let mut p = self.pos;
        while p < self.lines.len() {
            let line = self.lines[p].trim();
            if !line.is_empty() {
                return Some(line);
            }
            p += 1;
        }
        None
    }

    // ---- Hierarchy parsing ------------------------------------------------

    fn parse_hierarchy(&mut self) -> Result<(), BvhError> {
        let line = self.next_line()?;
        if line != "HIERARCHY" {
            return Err(self.err(format!("expected 'HIERARCHY', got '{line}'")));
        }

        let line = self.next_line()?;
        let name = line
            .strip_prefix("ROOT")
            .map(|s| s.trim())
            .ok_or_else(|| self.err(format!("expected 'ROOT <name>', got '{line}'")))?;

        self.parse_joint(name.to_string(), None, false)?;
        Ok(())
    }

    fn parse_joint(
        &mut self,
        name: String,
        parent: Option<usize>,
        is_end_site: bool,
    ) -> Result<usize, BvhError> {
        // Expect opening brace
        let line = self.next_line()?;
        if line != "{" {
            return Err(self.err(format!("expected '{{', got '{line}'")));
        }

        // Parse OFFSET
        let line = self.next_line()?;
        let offset = self.parse_offset(line)?;

        // For End Site nodes, there are no channels — just read closing brace
        let channels = if is_end_site {
            Vec::new()
        } else {
            let line = self.next_line()?;
            self.parse_channels(line)?
        };

        let joint_idx = self.joints.len();
        self.total_channels += channels.len();
        self.joints.push(BvhJoint {
            name,
            offset,
            channels,
            parent,
            is_end_site,
        });

        // Parse children until closing brace
        loop {
            let line = self.next_line()?;
            if line == "}" {
                break;
            }
            if let Some(child_name) = line.strip_prefix("JOINT") {
                self.parse_joint(child_name.trim().to_string(), Some(joint_idx), false)?;
            } else if line == "End Site" {
                let end_name = format!("{}_End", self.joints[joint_idx].name);
                self.parse_joint(end_name, Some(joint_idx), true)?;
            } else {
                return Err(self.err(format!(
                    "expected 'JOINT', 'End Site', or '}}', got '{line}'"
                )));
            }
        }

        Ok(joint_idx)
    }

    fn parse_offset(&self, line: &str) -> Result<[f32; 3], BvhError> {
        let rest = line
            .strip_prefix("OFFSET")
            .map(|s| s.trim())
            .ok_or_else(|| self.err(format!("expected 'OFFSET x y z', got '{line}'")))?;
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(self.err(format!("OFFSET needs 3 values, got {}", parts.len())));
        }
        let x = parts[0]
            .parse::<f32>()
            .map_err(|_| self.err(format!("invalid OFFSET x: '{}'", parts[0])))?;
        let y = parts[1]
            .parse::<f32>()
            .map_err(|_| self.err(format!("invalid OFFSET y: '{}'", parts[1])))?;
        let z = parts[2]
            .parse::<f32>()
            .map_err(|_| self.err(format!("invalid OFFSET z: '{}'", parts[2])))?;
        Ok([x, y, z])
    }

    fn parse_channels(&self, line: &str) -> Result<Vec<ChannelType>, BvhError> {
        let rest = line
            .strip_prefix("CHANNELS")
            .map(|s| s.trim())
            .ok_or_else(|| self.err(format!("expected 'CHANNELS ...', got '{line}'")))?;
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            return Err(self.err("CHANNELS line is empty"));
        }
        let count: usize = parts[0]
            .parse()
            .map_err(|_| self.err(format!("invalid channel count: '{}'", parts[0])))?;
        if parts.len() != count + 1 {
            return Err(self.err(format!(
                "CHANNELS declares {} but lists {} names",
                count,
                parts.len() - 1
            )));
        }
        let mut channels = Vec::with_capacity(count);
        for &name in &parts[1..] {
            let ch = match name {
                "Xposition" => ChannelType::Xposition,
                "Yposition" => ChannelType::Yposition,
                "Zposition" => ChannelType::Zposition,
                "Xrotation" => ChannelType::Xrotation,
                "Yrotation" => ChannelType::Yrotation,
                "Zrotation" => ChannelType::Zrotation,
                _ => return Err(self.err(format!("unknown channel type: '{name}'"))),
            };
            channels.push(ch);
        }
        Ok(channels)
    }

    // ---- Motion parsing ---------------------------------------------------

    fn parse_motion(&mut self) -> Result<(usize, f32, Vec<Vec<f32>>), BvhError> {
        let line = self.next_line()?;
        if line != "MOTION" {
            return Err(self.err(format!("expected 'MOTION', got '{line}'")));
        }

        // Frames: N
        let line = self.next_line()?;
        let frame_count: usize = line
            .strip_prefix("Frames:")
            .map(|s| s.trim())
            .ok_or_else(|| self.err(format!("expected 'Frames: N', got '{line}'")))?
            .parse()
            .map_err(|_| self.err(format!("invalid frame count in '{line}'")))?;

        // Frame Time: F
        let line = self.next_line()?;
        let frame_time: f32 = line
            .strip_prefix("Frame Time:")
            .map(|s| s.trim())
            .ok_or_else(|| self.err(format!("expected 'Frame Time: F', got '{line}'")))?
            .parse()
            .map_err(|_| self.err(format!("invalid frame time in '{line}'")))?;

        // Read frame data lines
        let mut frames = Vec::with_capacity(frame_count);
        for i in 0..frame_count {
            let line = self.next_line().map_err(|_| {
                self.err(format!(
                    "expected {} frames but file ended after {}",
                    frame_count, i
                ))
            })?;
            let values: Result<Vec<f32>, _> = line.split_whitespace().map(|s| s.parse()).collect();
            let values = values.map_err(|_| {
                self.err(format!("invalid float in frame data on line {}", self.pos))
            })?;
            if values.len() != self.total_channels {
                return Err(self.err(format!(
                    "frame {} has {} values, expected {} (total declared channels)",
                    i,
                    values.len(),
                    self.total_channels
                )));
            }
            frames.push(values);
        }

        Ok((frame_count, frame_time, frames))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a BVH motion capture file from a string.
pub fn parse_bvh(input: &str) -> Result<BvhClip, BvhError> {
    let mut parser = Parser::new(input);

    parser.parse_hierarchy()?;
    let (frame_count, frame_time, frames) = parser.parse_motion()?;

    Ok(BvhClip {
        joints: parser.joints,
        frame_time,
        frame_count,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BVH: &str = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
    JOINT Chest
    {
        OFFSET 0.0 5.21 0.0
        CHANNELS 3 Zrotation Xrotation Yrotation
        End Site
        {
            OFFSET 0.0 4.0 0.0
        }
    }
}
MOTION
Frames: 2
Frame Time: 0.033333
0.0 35.0 0.0 -2.1 0.5 1.3 0.1 -0.2 0.3
0.0 35.1 0.0 -2.0 0.4 1.2 0.2 -0.1 0.4
";

    #[test]
    fn parse_sample() {
        let clip = parse_bvh(SAMPLE_BVH).unwrap();
        assert_eq!(clip.joints.len(), 3); // Hips, Chest, Chest_End
        assert_eq!(clip.frame_count, 2);
        assert!((clip.frame_time - 0.033333).abs() < 1e-5);
        assert_eq!(clip.frames.len(), 2);
        assert_eq!(clip.frames[0].len(), 9); // 6 + 3
    }

    #[test]
    fn joint_structure() {
        let clip = parse_bvh(SAMPLE_BVH).unwrap();
        let hips = &clip.joints[0];
        assert_eq!(hips.name, "Hips");
        assert_eq!(hips.parent, None);
        assert_eq!(hips.channels.len(), 6);
        assert!(!hips.is_end_site);

        let chest = &clip.joints[1];
        assert_eq!(chest.name, "Chest");
        assert_eq!(chest.parent, Some(0));
        assert_eq!(chest.channels.len(), 3);

        let end = &clip.joints[2];
        assert_eq!(end.name, "Chest_End");
        assert_eq!(end.parent, Some(1));
        assert!(end.is_end_site);
        assert!(end.channels.is_empty());
    }

    #[test]
    fn channel_offsets() {
        let clip = parse_bvh(SAMPLE_BVH).unwrap();
        assert_eq!(clip.joint_channel_offset(0), 0);
        assert_eq!(clip.joint_channel_offset(1), 6);
        assert_eq!(clip.joint_channel_offset(2), 9);
    }

    #[test]
    fn duration() {
        let clip = parse_bvh(SAMPLE_BVH).unwrap();
        assert!((clip.duration() - 0.033333).abs() < 1e-5);
    }

    #[test]
    fn bad_channel_count() {
        let bad = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
}
MOTION
Frames: 1
Frame Time: 0.033333
1.0 2.0 3.0
";
        let err = parse_bvh(bad).unwrap_err();
        assert!(err.message.contains("values, expected"));
    }
}
