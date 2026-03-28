# Changelog

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
