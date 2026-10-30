# Changelog

## [0.8.0] - 2026-10-30

### Fixed
- RPC screenshots are ordered render barriers and return only after PNG completion; concurrent requests no longer overwrite one another. Failed loads preserve the previous scene.
- `--no-vsync` selects a supported present mode; native PNG readback correctly converts BGRA to RGBA.
- Alpha cutouts apply to backface diagnostics, including authored alpha with viewer color overrides.
- Static angle-only sprite sheets no longer require animation clips. Camera updates preserve orbit angles and exact pole views.
- Skinning is centered after deformation, and bind-pose bounds account for dequantization in inverse bind matrices. Arbitrary joint order and non-joint ancestors are preserved.
- **Per-primitive glTF materials** (bn-uhc): each primitive is drawn as its own batch with its own material instead of every primitive using the first `baseColorTexture`. Applies to `view`, `view --remote`, `render`, and sprite sheets.
- `doubleSided` materials are no longer back-face culled; `alphaMode` `MASK` (with `alphaCutoff`) and `BLEND` are honored, with blended materials drawn after opaque ones.

### Added
- Independent RPC ports (`--port`, including ephemeral port 0), window labels, initial sizing, atomic camera controls and ground-shadow RPC toggles.
- `msh serve` for persistent windowless captures at exact dimensions; ordered `shoot` batches and integer material/backface pixel diagnostics.
- Raw and position-welded topology analysis with explicit tolerance, boundary, manifold and closed-surface results.
- Simultaneous scene meshes via `--mesh all`, external clips via `--clips`, and RPC clip/time/frame controls.
- Meshopt-compressed and quantized glTF decoding, with per-texture UV transforms and decoded-size/span validation.
- Directional shadows on an invisible ground catcher, hidden from below; G toggles the shadow.
- Normal maps, metallic-roughness, and occlusion textures; vertex normals and tangents are used for lit shading (skinned along with positions).
- Hemisphere ambient fill in `lit` shading so PBR assets stay readable on their unlit sides.
- Mipmapped, anisotropically filtered material textures.

### Changed
- RPC `load_model` and `screenshot` now return structured completion results rather than queued-status strings.
- Default background is now a dithered radial gradient (medium-dark cool gray center to dark gray edges) instead of solid black, in `view` and `render`. `--clear-color`, `--transparent-bg`, `remote set-clear-color`, and `--preset sprite-bake` still give a solid background.

## [0.7.0] - 2026-04-05

### Added
- Mesh cleanup and simplification, projection onto a reference surface, and optional bone-weight transfer.
- GLB output and multi-mesh viewer selection.

## [0.6.0] - 2026-03-28

### Added
- **Skeleton & animation support**: load and play back glTF skeletal animations with GPU skinning.
- **BVH motion capture overlay**: `--bvh` flag on `msh view` maps BVH motion data onto mesh skeletons with automatic joint name matching.
- **Standalone BVH viewer**: `msh view file.bvh` renders skeleton motion capture files directly.
- **Sprite sheet rendering**: `msh render --sprite-sheet` composites animation frames and rotation angles into a single atlas PNG.
- **Animation frame rendering**: `--frame`, `--frames` (with optional step, e.g. `0-39:4`), and `--angles` flags for batch rendering animation sequences.
- **Texture rendering**: GLB `baseColorTexture` with UV mapping is now rendered.
- **`--scale` flag**: uniform scale multiplier for `view` and `render` commands.
- **`--no-center` flag**: skip auto-centering the mesh at the origin for `view` and `render`.
- **`--animation` flag**: select animation clip by name or index for `view` and `render`.
- **`--angle-offset` flag**: rotate all angle steps by a fixed offset in degrees.

## [0.5.1] - 2026-03-21

### Fixed
- GLB material color detection: meshes where all materials share the same `baseColorFactor` now correctly fall back to uniform `base_color` instead of rendering with per-vertex colors. Fixes `--preset sprite-bake` overriding issue.

### Added
- `--camera-pos x,y,z` and `--camera-target x,y,z` flags for `msh render`, enabling fixed camera angles for sprite baking.

## [0.5.0] - 2026-03-21

### Added
- **Headless rendering**: `msh render` command renders to PNG without opening a window. Supports all render options (`--preset`, `--shading`, `--base-color`, `--width`, `--height`, etc.).
- **Per-face material colors**: meshes with multiple materials now render with distinct colors instead of a single global base color.
  - OBJ+MTL: parses `.mtl` sidecar files for `Kd` diffuse colors via `usemtl` groups.
  - GLB/glTF: extracts `baseColorFactor` from PBR materials per primitive.
  - 3MF: reads `colorgroup` definitions with per-triangle `pid`/`p1` attributes.
- **3MF file format support**: load `.3mf` files (ZIP-archived XML) with geometry and optional per-triangle colors.
- **`--z-up` flag**: converts Z-up coordinates to Y-up for meshes from OpenSCAD and other CAD tools.
- **Render state controls**: projection modes (perspective/ortho), shading modes (lit/flat/unlit), configurable clear color, base color, and light direction via CLI flags and JSON-RPC.
- **Render presets**: `--preset viewer` and `--preset sprite-bake` bundle common render settings.
- **RPC methods**: `set_projection`, `set_clear_color`, `set_shading`, `set_base_color`, `set_light_direction`, `apply_preset`.

### Changed
- Vertex buffer now includes per-vertex color attribute (position + RGBA).
- Shader uses per-vertex color when materials are present, falls back to uniform `base_color` otherwise.

## [0.4.2] - 2025

- Fix remote rotation.

## [0.4.1] - 2025

- Add vsync toggle.

## [0.4.0] - 2025

- Cross-platform fonts.
- Migrate to wgpu rendering backend.
