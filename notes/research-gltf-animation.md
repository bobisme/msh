# Research: gltf Crate Animation API (v1.4.1)

## API Surface

### Core Types

The `gltf::animation` module exposes:

- **`Animation<'a>`** — Top-level animation object. Has `index()`, `name()`, `channels()`, `samplers()`.
- **`Channel<'a>`** — Binds a sampler to a target node+property. Has `sampler()`, `target()`, `reader(get_buffer_data)`.
- **`Sampler<'a>`** — Defines the keyframe graph: `input()` (timestamps accessor), `output()` (values accessor), `interpolation()`.
- **`Target<'a>`** — The animated node and property: `node()` -> `Node`, `property()` -> `Property`.
- **`Property`** enum — `Translation`, `Rotation`, `Scale`, `MorphTargetWeights`.
- **`Interpolation`** enum — `Linear`, `Step`, `CubicSpline`.

### Iteration Pattern

```rust
let (document, buffers, _images) = gltf::import(path)?;

for anim in document.animations() {
    println!("Animation {:?}", anim.name());
    for channel in anim.channels() {
        let target = channel.target();
        let node = target.node();
        let property = target.property();  // Translation | Rotation | Scale | MorphTargetWeights
        let sampler = channel.sampler();
        let interpolation = sampler.interpolation();  // Linear | Step | CubicSpline

        let reader = channel.reader(|buf| Some(&buffers[buf.index()]));
        let timestamps: Vec<f32> = reader.read_inputs().unwrap().collect();

        match reader.read_outputs().unwrap() {
            ReadOutputs::Translations(iter) => { /* Vec<[f32; 3]> */ }
            ReadOutputs::Rotations(iter) => {
                let quats: Vec<[f32; 4]> = iter.into_f32().collect();
            }
            ReadOutputs::Scales(iter) => { /* Vec<[f32; 3]> */ }
            ReadOutputs::MorphTargetWeights(iter) => {
                let weights: Vec<f32> = iter.into_f32().collect();
            }
        }
    }
}
```

### util Module — Reader

`Channel::reader()` takes a closure `Fn(Buffer) -> Option<&[u8]>` and returns a `Reader` with:
- `read_inputs()` -> `Option<ReadInputs>` (iterator of `f32` timestamps)
- `read_outputs()` -> `Option<ReadOutputs>` (enum with typed iterators)

`ReadOutputs` enum variants:
- `Translations(Translations)` — yields `[f32; 3]`
- `Rotations(Rotations)` — yields rotation data; call `.into_f32()` to get `[f32; 4]` (XYZW quaternions)
- `Scales(Scales)` — yields `[f32; 3]`
- `MorphTargetWeights(MorphTargetWeights)` — call `.into_f32()` to get `f32`

**Important:** `Rotations` may be stored as normalized integers (i8/u8/i16/u16) in the accessor. The `.into_f32()` adapter handles the conversion transparently.

## Reading Keyframe Data

### Timestamps (input accessor)

- Always `f32` scalars, in seconds.
- Monotonically increasing.
- The input accessor `min`/`max` give the animation time range.
- Multiple samplers within an animation can share the same input accessor.

### Output values

For **Linear** and **Step**: output count == input count (one value per keyframe).

For **CubicSpline**: output count == 3 * input count. The data is packed as triplets per keyframe:
```
[in_tangent_0, value_0, out_tangent_0, in_tangent_1, value_1, out_tangent_1, ...]
```

**The gltf crate Reader does NOT unpack CubicSpline data** — it returns all 3N values in order. The consumer must interpret every 3rd element starting at index 1 as the actual keyframe value, with adjacent elements being tangents.

### Accessor-level access (alternative)

You can also go through `sampler.input()` / `sampler.output()` to get raw `Accessor` objects:
- `accessor.count()` — number of elements
- `accessor.data_type()` — e.g., `F32`
- `accessor.dimensions()` — e.g., `Scalar`, `Vec3`, `Vec4`

This is useful for validation but the Reader API is preferred for actual data extraction.

## CubicSpline Tangent Packing

Per the glTF 2.0 spec, for CubicSpline interpolation the output accessor stores **3 elements per keyframe** in the order:

1. **In-tangent** (a_k) — tangent approaching this keyframe
2. **Value** (v_k) — the keyframe value itself
3. **Out-tangent** (b_k) — tangent leaving this keyframe

For interpolation between keyframes k and k+1:
```
t_delta = t_{k+1} - t_k
t = (current_time - t_k) / t_delta

p(t) = (2t^3 - 3t^2 + 1) * v_k
      + (t^3 - 2t^2 + t) * t_delta * b_k
      + (-2t^3 + 3t^2) * v_{k+1}
      + (t^3 - t^2) * t_delta * a_{k+1}
```

The tangents must be **scaled by t_delta** before use in the cubic Hermite formula.

### Spike validation

The InterpolationTest.glb model confirmed:
- CubicSpline output accessor count is exactly 3x the input count for all property types
- For "CubicSpline Translation" (5 keyframes): 15 output Vec3 values
- For "CubicSpline Rotation" (5 keyframes): 15 output Vec4 values
- For "CubicSpline Scale" (5 keyframes): 15 output Vec3 values

Pattern in the output for CubicSpline Translation with 5 keyframes:
```
[0] in_tangent_0 = (0, 0, 0)
[1] value_0      = (3.4, 6.8, 0)     <-- actual keyframe value
[2] out_tangent_0 = (0, 0, 0)
[3] in_tangent_1 = (0, 0, 0)
[4] value_1      = (3.4, 10.8, 0)    <-- actual keyframe value
[5] out_tangent_1 = (0, 0, 0)
...
```

## Quaternion Normalization

The glTF spec states rotation quaternions **must** be unit length, and implementations should normalize after interpolation.

### Spike findings

- RiggedSimple.glb: max quaternion norm deviation = 0.00000006 (< 1e-5) — well-normalized in source data
- InterpolationTest.glb: max deviation = 0.00000006 — also well-normalized

In practice, source data is well-normalized, but **after linear interpolation (slerp) or cubic spline evaluation, the result must be re-normalized**. This is especially important for CubicSpline rotation where the cubic Hermite formula does not preserve unit length.

## Multiple Animations

Animations are distinguished by:
- **Index** — `animation.index()` (0-based, matches JSON array position)
- **Name** — `animation.name()` (optional, `Option<&str>`)

RiggedSimple.glb has 1 unnamed animation. InterpolationTest.glb has 9 named animations ("Step Scale", "Linear Scale", "CubicSpline Scale", etc.). Names are only available when the `names` feature is enabled on the gltf crate (already enabled in msh).

Channels within an animation each target a specific node+property combination. A single animation typically has multiple channels (e.g., T+R+S for one bone), and multiple animations can target the same nodes (for blending/selection at runtime).

## API Quirks and Observations

1. **Channel index is local to the animation**, not global. `channel.index()` returns the position within `anim.channels()`.

2. **Samplers can be shared** — multiple channels in the same animation may reference the same sampler (same input accessor). In RiggedSimple.glb, all 3 channels (T/R/S) share the same input accessor (index 5).

3. **Rotations need `.into_f32()`** — the `ReadOutputs::Rotations` variant yields a `Rotations` iterator that may contain normalized integer data. Always call `.into_f32()` to get `[f32; 4]`.

4. **MorphTargetWeights also need `.into_f32()`** — same pattern as rotations.

5. **CubicSpline Reader does not separate tangents from values** — the Reader returns all 3N elements flat. Consumer code must stride by 3: `values[i*3+0]` = in_tangent, `values[i*3+1]` = keyframe value, `values[i*3+2]` = out_tangent.

6. **Property vs ReadOutputs mismatch is impossible** — the Reader dispatches based on the property type from the channel target. `Property::Rotation` always yields `ReadOutputs::Rotations`, etc.

7. **No built-in interpolation** — the gltf crate only provides data extraction. Interpolation between keyframes (linear lerp, slerp for quaternions, cubic Hermite) must be implemented by the consumer.

8. **Node targets reference skeleton joints** — for skeletal animation, the channel target node is a joint in the skin's joint list. The node index from `target.node().index()` must be mapped to the joint index in the skin's `joints()` array.

## Extraction Pattern for msh Integration

```rust
use gltf::animation::util::ReadOutputs;

struct Keyframe<T> {
    time: f32,
    value: T,
    // For CubicSpline:
    in_tangent: Option<T>,
    out_tangent: Option<T>,
}

struct AnimationChannel {
    node_index: usize,
    property: Property,
    interpolation: Interpolation,
    keyframes: Vec<Keyframe<Vec<f32>>>,  // flattened values
}

fn extract_animations(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Vec<(Option<String>, Vec<AnimationChannel>)> {
    document.animations().map(|anim| {
        let name = anim.name().map(String::from);
        let channels = anim.channels().filter_map(|channel| {
            let target = channel.target();
            let reader = channel.reader(|buf| Some(&buffers[buf.index()]));
            let timestamps: Vec<f32> = reader.read_inputs()?.collect();
            let interp = channel.sampler().interpolation();
            let is_cubic = matches!(interp, Interpolation::CubicSpline);

            let outputs = reader.read_outputs()?;
            // ... extract typed data, handle CubicSpline stride-3 packing
            // Return AnimationChannel
            None // placeholder
        }).collect();
        (name, channels)
    }).collect()
}
```

## Test Assets Used

- `notes/spikes/test-assets/RiggedSimple.glb` — 1 unnamed animation, 3 channels (T/R/S), 50 keyframes, Linear interpolation, single animated bone
- `notes/spikes/test-assets/InterpolationTest.glb` — 9 named animations covering all 3 interpolation modes x 3 property types (T/R/S), 5 keyframes each
