# Changelog

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
