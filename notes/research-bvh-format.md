# BVH Motion Capture Format: Research Notes

Research bone: bn-2jm

## BVH Format Specification Summary

BVH (Biovision Hierarchy) is a plain-text motion capture file format originally created by Biovision. It is the de facto standard for distributing motion capture data due to its simplicity and wide tool support. A BVH file consists of two sections: **HIERARCHY** and **MOTION**.

### HIERARCHY Section

Defines the skeleton as a tree of joints. Begins with the keyword `HIERARCHY`, followed by a `ROOT` declaration.

```
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
    JOINT Chest
    {
        OFFSET 0.0 5.21 0.0
        CHANNELS 3 Zrotation Xrotation Yrotation
        JOINT Neck
        {
            OFFSET 0.0 8.59 0.0
            CHANNELS 3 Zrotation Xrotation Yrotation
            End Site
            {
                OFFSET 0.0 4.0 0.0
            }
        }
    }
}
```

Key elements:
- **ROOT**: The single root joint (typically Hips). Has both position and rotation channels (usually 6 channels).
- **JOINT**: Child joints. Typically have 3 rotation channels only.
- **End Site**: Leaf nodes with only an OFFSET (no channels). Represent the terminal bone endpoint.
- **OFFSET**: Rest-pose position relative to the parent joint (X, Y, Z).
- **CHANNELS**: Number of DOFs followed by channel type labels. The order of labels defines the order of values in the motion data.

### MOTION Section

Contains per-frame animation data as a flat table of floats.

```
MOTION
Frames: 120
Frame Time: 0.033333
0.0 35.0 0.0 -2.1 0.5 1.3 ...
0.0 35.1 0.0 -2.0 0.4 1.2 ...
```

- **Frames**: Total number of motion samples.
- **Frame Time**: Duration of each frame in seconds (e.g., 0.033333 = ~30 fps).
- Each subsequent line is one frame. Values appear in the order channels were declared during hierarchy traversal (depth-first).

## Channel Ordering Conventions

Channel types are: `Xposition`, `Yposition`, `Zposition`, `Xrotation`, `Yrotation`, `Zrotation`.

The most common convention:
- **Root joint**: 6 channels — `Xposition Yposition Zposition Zrotation Xrotation Yrotation`
- **Other joints**: 3 channels — `Zrotation Xrotation Yrotation`

However, the channel order is **per-joint configurable** — the CHANNELS line explicitly lists the order for each joint. A parser must respect the declared order, not assume ZXY. Some BVH files use different orderings (e.g., XYZ or YZX), though ZXY is by far the most common.

## Euler Angle Convention

This is the trickiest part of BVH and a common source of bugs.

### Rotation Order

The standard BVH rotation order is **Z, X, Y** — meaning Z is applied first, then X, then Y. The channels listed first in the CHANNELS declaration are applied first to build the local rotation.

### Intrinsic vs. Extrinsic

BVH rotations are **intrinsic** (rotating around the local/body axes, not the fixed world axes). Each successive rotation is applied around the axis as it has been reoriented by previous rotations.

In matrix form, for the common `Zrotation Xrotation Yrotation` order, the local rotation matrix is:

```
R_local = R_Y * R_X * R_Z
```

This is because with intrinsic rotations, the matrix for the last-applied rotation (Y) goes on the left when using column-vector convention. Equivalently, this is the same as **extrinsic YXZ** (since intrinsic ZXY = extrinsic YXZ with reversed order).

### Coordinate System

BVH uses a **right-handed coordinate system** with **Y-up**. This aligns with glTF's convention, which simplifies integration.

### Building the World Transform

For any joint, the world transform is computed by walking up the hierarchy:

```
T_world = T_parent_world * Translate(offset) * R_local
```

Where `offset` is the joint's rest-pose OFFSET and `R_local` is constructed from the frame's Euler angles per the channel order.

## Rust Crate Evaluation

### bvh_anim (v0.4.0)

- **Repository**: github.com/burtonageo/bvh_anim
- **License**: MIT
- **Last release**: May 2019 (6+ years ago)
- **Downloads**: ~22/month on crates.io
- **Stars**: 10
- **Status**: Self-described "alpha quality software"

**API surface**:
- `Bvh` struct with `from_reader()`, `from_bytes()` for parsing
- `Joint`, `Channel`, `Frame` types with iterators (`Joints`, `Frames`)
- `ChannelType` enum covering all 6 DOF types
- `bvh!` macro for compile-time BVH construction
- Write support via `write_to()` and `WriteOptions`
- FFI bindings (optional feature)
- Builder pattern for dynamic construction

**Pros**:
- Most complete BVH crate in the Rust ecosystem
- Clean API design with proper iterators
- Supports both reading and writing
- MIT license, no concerns
- Uses `mint` for math types (interop-friendly)

**Cons**:
- Unmaintained for 6+ years
- Depends on outdated crate versions (bstr 0.1, smallvec 0.6, lexical 2.x)
- Alpha quality — may have edge-case parsing bugs
- No forward kinematics computation
- 292 commits suggests reasonable development effort, but stalled

### bvh_anim_parser (v1.0.1)

- **Repository**: github.com/wojtekpachowiak/bvh-anim-parser
- **License**: MIT
- **Last release**: May 2024
- **Downloads**: ~66/month
- **Size**: 698 SLoC

**API surface**:
- `BvhMetadata` and `BvhData` structs
- Computes forward kinematics (global joint positions and rotations)
- Optional Bevy visualization feature
- Uses `cgmath` and `regex`

**Pros**:
- More recently maintained
- Computes global transforms (forward kinematics), not just raw parsing
- Reached 1.0

**Cons**:
- Depends on `cgmath` (which is itself in maintenance mode; `glam` is now the Rust standard)
- Uses `regex` for parsing (unusual and potentially fragile for a structured format)
- Depends on Bevy 0.13 for visualization (heavy optional dep)
- Small user base, single author
- No write support

### Other Crates

- **`bvh` (svenstaro/bvh)**: This is a Bounding Volume Hierarchy crate for ray tracing — NOT a motion capture parser. Name collision.
- **`skeletal_animation`**: A data-driven skeletal animation library, not a BVH parser. Uses JSON for animation definitions.

No other Rust crates exist specifically for BVH motion capture parsing.

## Recommendation: Hand-Roll Parser

**Recommendation: Write a minimal hand-rolled BVH parser for msh.**

Justification:

1. **BVH is a trivially simple format.** The entire parser is ~200-300 lines of Rust. The hierarchy section is a recursive descent parse of a small grammar (ROOT/JOINT/End Site with OFFSET and CHANNELS). The motion section is a flat grid of floats. There is no compression, no binary encoding, no complex escaping.

2. **Neither existing crate is a good fit.**
   - `bvh_anim` has severely outdated dependencies that would conflict with modern Rust ecosystem crates and is unmaintained. Forking and updating it would be more work than writing a fresh parser.
   - `bvh_anim_parser` uses `cgmath` and `regex`, adding unwanted dependencies. msh likely wants to use `glam` for math.

3. **We need tight integration with our own types.** We'll want to parse directly into msh's internal skeleton/animation representation, converting Euler angles to quaternions on load and building the joint hierarchy in our own format. A hand-rolled parser lets us do this in a single pass with zero intermediate allocations.

4. **We only need read support.** msh will never need to write BVH files, eliminating half the value proposition of `bvh_anim`.

5. **Control over error handling.** We can produce clear, msh-specific error messages (e.g., "BVH joint 'LeftArm' on line 42: expected 3 channels, found 6").

## Joint Name Mapping Challenge

BVH files have no standardized joint naming convention. Different sources use completely different names for the same bones.

### Common Naming Conventions by Source

| Body Part | CMU (MotionBuilder BVH) | Mixamo | glTF (typical) |
|-----------|------------------------|--------|-----------------|
| Root/Pelvis | Hips | mixamorig:Hips | Hips |
| Spine lower | LowerBack | mixamorig:Spine | Spine |
| Spine mid | Spine | mixamorig:Spine1 | Spine1 |
| Spine upper | Spine1 | mixamorig:Spine2 | Spine2 |
| Neck | Neck / Neck1 | mixamorig:Neck | Neck |
| Head | Head | mixamorig:Head | Head |
| L Shoulder | LeftShoulder | mixamorig:LeftShoulder | LeftShoulder |
| L Upper Arm | LeftArm | mixamorig:LeftArm | LeftArm |
| L Forearm | LeftForeArm | mixamorig:LeftForeArm | LeftForeArm |
| L Hand | LeftHand | mixamorig:LeftHand | LeftHand |
| L Thigh | LeftUpLeg | mixamorig:LeftUpLeg | LeftUpLeg |
| L Shin | LeftLeg | mixamorig:LeftLeg | LeftLeg |
| L Foot | LeftFoot | mixamorig:LeftFoot | LeftFoot |
| L Toe | LeftToeBase | mixamorig:LeftToeBase | LeftToeBase |

Key observations:
- **Mixamo** prefixes all joints with `mixamorig:` — easy to strip and then names mostly match MotionBuilder convention
- **CMU** uses MotionBuilder-compatible names for most joints, but spine naming differs (LowerBack/Spine/Spine1 vs Spine/Spine1/Spine2)
- **Spine segment count varies**: CMU has 3 spine joints, Mixamo has 3, but the names differ
- **Finger joints**: CMU marks them as non-captured (LFingers, RFingers, LThumb, RThumb with no real data). Mixamo includes full finger articulation (Thumb1-3, Index1-3, etc.)
- **Neck segments**: CMU sometimes has Neck + Neck1; Mixamo has just Neck

### Mapping Strategies

1. **Exact name match** (after stripping known prefixes like `mixamorig:`): Works for Mixamo-to-MotionBuilder-convention targets.
2. **Canonical name table**: Map known aliases to canonical bone IDs (e.g., `LowerBack` -> `Spine`, `Chest` -> `Spine1`).
3. **Topology-based matching**: Match by skeleton structure (bone count per chain, hierarchy depth) rather than names. More robust but significantly more complex.

## MVP Scope Recommendation

**Recommendation: Start with matching-skeleton-only playback, with simple name normalization.**

### What "matching skeleton only" means

The BVH file's skeleton hierarchy must match the target glTF skeleton — same number of joints, same parent-child relationships, and compatible names (after normalization). No retargeting, no bone remapping, no proportion adjustment.

### MVP scope

1. **Parse BVH**: Hand-rolled parser producing joint hierarchy + per-frame Euler angles
2. **Name normalization**: Strip known prefixes (`mixamorig:`), case-insensitive matching, and a small hardcoded alias table for the most common variants (e.g., `LowerBack` -> `Spine`)
3. **Euler-to-quaternion conversion**: Convert per the declared channel order (not hardcoded ZXY)
4. **Matching validation**: Verify the BVH skeleton matches the target glTF skeleton (joint count, hierarchy structure) and report clear errors on mismatch
5. **Playback**: Apply BVH transforms to glTF joints by name

### Why not retargeting for MVP

- **Retargeting is a deep rabbit hole.** Even basic retargeting requires solving: different bone counts, different rest poses, different proportions, different coordinate conventions, IK for foot placement, etc.
- **Matching skeletons cover the most common use case.** Users typically export both the mesh and animation from the same tool (Mixamo, Blender), so skeletons naturally match.
- **Name normalization handles 80% of "almost matching" cases.** Stripping `mixamorig:` prefixes and aliasing common spine name variants covers the Mixamo and CMU databases, which are the two largest BVH sources.
- **Clean foundation for future retargeting.** A well-structured matching implementation (with clear joint-identity resolution) is the right foundation to layer retargeting on later.
