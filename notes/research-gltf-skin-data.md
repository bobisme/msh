# Research: gltf crate v1.4.1 skin data API

Spike code: `notes/spikes/gltf-skin-spike/` (Cargo project)
Test asset: `notes/spikes/test-assets/RiggedSimple.glb` (Khronos sample, 2 joints, 160 verts)

## API surface

### Key types and entry points

- `gltf::Document::skins()` -> iterator of `Skin<'a>`
- `gltf::Node::skin()` -> `Option<Skin<'a>>` (the skin bound to a node)
- `gltf::Skin` — the central skin type:
  - `.joints()` -> iterator of `Node<'a>` (the joint nodes, ordered by joint index)
  - `.inverse_bind_matrices()` -> `Option<Accessor<'a>>`
  - `.skeleton()` -> `Option<Node<'a>>` (skeleton root; `None` means resolve to scene root)
  - `.name()` -> `Option<&str>` (requires `names` feature)
  - `.reader(get_buf)` -> `skin::Reader` with `.read_inverse_bind_matrices()` -> `Option<Iter<[[f32;4];4]>>`

### Per-vertex joint/weight data (on mesh primitives)

- `primitive.reader(get_buf).read_joints(set)` -> `Option<ReadJoints<'a>>`
  - `ReadJoints` is an enum: `U8(Iter<[u8;4]>)` | `U16(Iter<[u16;4]>)`
  - Cast uniformly with `.into_u16()` -> iterator of `[u16; 4]`
  - `set` param supports multi-set skinning (JOINTS_0, JOINTS_1, ...) — most assets only have set 0
- `primitive.reader(get_buf).read_weights(set)` -> `Option<ReadWeights<'a>>`
  - `ReadWeights` is an enum: `U8(Iter<[u8;4]>)` | `U16(Iter<[u16;4]>)` | `F32(Iter<[f32;4]>)`
  - Cast uniformly with `.into_f32()` -> iterator of `[f32; 4]`
- Raw accessor metadata available via `primitive.get(&Semantic::Joints(0))` / `Semantic::Weights(0)`

### Node hierarchy (for joint trees)

- `Node::children()` -> iterator of child `Node`s
- **No `Node::parent()` method** — you must build a parent map yourself by iterating `document.nodes()` and recording children
- `Node::transform()` -> `Transform` enum: either `Decomposed { translation, rotation, scale }` or `Matrix { matrix: [[f32;4];4] }`
- `Node::index()` -> `usize` (stable index for lookups and the parent map)

## Accessor layout

### JOINTS_0

- **Dimensions:** Vec4 (4 joint indices per vertex)
- **Component type:** U8 or U16 (RiggedSimple uses U16)
- **Normalized:** false
- **Count:** equals vertex count
- Values are indices into the skin's `joints()` array (not node indices)

### WEIGHTS_0

- **Dimensions:** Vec4 (4 weights per vertex)
- **Component type:** U8, U16, or F32 (RiggedSimple uses F32)
- **Normalized:** false for F32; U8/U16 would be normalized to [0,1]
- **Count:** equals vertex count
- Weights sum to 1.0 per vertex (confirmed in spike output)

### Multi-set skinning

- `read_joints(1)` / `read_weights(1)` returns `None` for RiggedSimple (only set 0)
- For models needing >4 influences per vertex, JOINTS_1/WEIGHTS_1 etc. would be present

## Inverse bind matrix extraction

```rust
let reader = skin.reader(|buf| Some(&buffers[buf.index()]));
if let Some(ibm_iter) = reader.read_inverse_bind_matrices() {
    let ibms: Vec<[[f32; 4]; 4]> = ibm_iter.collect();
    // ibms[i] corresponds to skin.joints().nth(i)
}
```

- Returns `Option<Iter<[[f32;4];4]>>` — `None` means identity matrices (pre-applied)
- Matrix layout: column-major (matches glTF spec), each `[f32;4]` is a column
- Count equals number of joints

## Joint parent-child traversal

The gltf crate does not provide a `parent()` method. Build the map manually:

```rust
let mut parent_map: HashMap<usize, usize> = HashMap::new();
for node in document.nodes() {
    for child in node.children() {
        parent_map.insert(child.index(), node.index());
    }
}
```

To find the skin on a mesh node: iterate `document.nodes()`, check `node.skin().is_some()` and `node.mesh().is_some()`.

## Sparse accessor support

- The `Accessor` type exposes `.sparse()` -> `Option<sparse::Sparse>` for metadata
- `accessor::Iter` handles sparse accessors transparently via a `SparseIter` variant — no special handling needed on our side
- RiggedSimple has no sparse accessors, but the crate will correctly overlay sparse values if present
- **Implication:** our loader code can use `read_joints()` / `read_weights()` / `read_inverse_bind_matrices()` without worrying about sparse vs dense

## Spike results

### What worked

- `gltf::import()` loads everything (document, buffers, images) in one call — same pattern as existing loader.rs
- `skin.joints()` returns nodes in joint-index order, matching IBMs positionally
- `read_joints(0).into_u16()` and `read_weights(0).into_f32()` provide uniform typed access regardless of underlying component type
- Joint indices in JOINTS_0 are indices into the `skin.joints()` array, not raw node indices
- All weight sums confirmed to be exactly 1.0

### Surprising / noteworthy

1. **No parent accessor on Node** — must build parent map manually. This is a common pitfall.
2. **Joint node transforms can be raw matrices** — Bone #3 (root joint) had a `Transform::Matrix` not `Decomposed`. Must handle both variants in animation code.
3. **skeleton() can be None** — spec says joints resolve to scene root in that case. RiggedSimple does set it.
4. **Joint indices are NOT node indices** — joint index `1` in JOINTS_0 maps to `skin.joints().nth(1)`, which is node #4 in RiggedSimple. Code must map through the joints array.
5. **Weights are always Vec4** — even if a vertex is influenced by fewer joints, remaining slots are zero-weighted (joint index 0, weight 0.0).

### API quirks

- `skin.reader()` only provides `read_inverse_bind_matrices()`. Joint/weight data lives on the *mesh primitive* reader, not the skin reader. This split means you need to correlate `node.skin()` with `node.mesh()` yourself.
- The `reader()` closure is `Fn(Buffer) -> Option<&[u8]>` — must be cloneable, cannot hold `&mut` state.
- `primitive.reader()` is consumed per-read (the reader borrows the closure). For multiple reads from the same primitive, create the reader multiple times or clone it.
