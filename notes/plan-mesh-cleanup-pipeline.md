# Mesh Cleanup Pipeline: PaMO Replacement in msh

## Goal

Replace the Python/CUDA PaMO pipeline (`tools/pamo-clean`) with native Rust
mesh cleanup integrated into msh. The current PaMO setup requires a patched
Python venv with CUDA 13.1 workarounds and 8 patches. A native implementation
eliminates the dependency and integrates into the asset pipeline directly.

Our meshes are small: 500-4,000 vertices. PaMO was designed for million-poly
scan decimation. We only need a fraction of its capability.

## Current Pipeline (validated on game assets)

```
msh remesh voxel input.glb -o stage1.obj --method manifold --size 0.004
msh remesh simplify stage1.obj -o output.obj --target-verts 2000
```

Two stages. The voxel step is required — game assets have non-manifold topology
(UV seams, separate pieces, open boundaries) that crashes direct QEM. Voxel
remeshing at 0.004 (~250^3 effective resolution) produces a clean manifold mesh
that QEM can safely decimate.

### What we learned from testing

**Direct QEM on game assets doesn't work.** Even with `--keep-boundary`, it
either barely reduces (28K → 23K) or shatters the mesh. These models have
non-manifold topology that baby_shark's QEM can't handle for aggressive
decimation. The voxel step is not optional.

**High-res voxelization + QEM produces good results.** At voxel size 0.004, the
SDF captures fine detail. QEM then reduces cleanly to any target. Tested on
barracks, rifleman, mineral, scout, worker — all produce usable LODs.

**Projection (Stage 3) adds minimal value at high voxel resolution.** With
0.004 voxel size, QEM vertices are already within 0.001-0.002 of the original
surface. Projection barely moves anything. It would matter more at coarser
voxel sizes.

**The auto-fallback in `remesh simplify` works.** It tries aggressive QEM first,
catches panics from non-manifold topology, and falls back to `--keep-boundary`.
This means `remesh simplify` is safe to call on any input.

### Tested results (scout model, 28K verts → LODs)

| Target | Output verts | Output faces | Quality |
|--------|-------------|-------------|---------|
| 8000 | 7,455 | 15,607 | Excellent |
| 4000 | 3,456 | 7,609 | Good |
| 2000 | 1,457 | 3,611 | Good |
| 1000 | 1,072 | 2,841 | Acceptable |

## What's Implemented

### `msh remesh simplify` (done)

Wraps baby_shark's QEM decimator. Accepts `--target-verts`, `--ratio`,
`--max-error`, `--keep-boundary`. Auto-falls back to `--keep-boundary` if
aggressive QEM panics on non-manifold input.

### `msh remesh project` (done, two backends)

1. **del-msh solver** (default): Uses `del_msh_core::trimesh3_move_avoid_intersection`
   for gradient descent with log-barrier collision avoidance + CCD line search.
   Falls back to direct blend if mesh has self-intersections.

2. **Heuristic** (old): Iterative closest-point projection with Laplacian
   regularization. Available but not used by default — the crease snapping was
   too aggressive and made meshes worse.

### Projection module (`src/mesh/projection.rs`)

- Closest-point-on-triangle (Ericson algorithm, all 7 Voronoi regions)
- Spatial hash grid for fast nearest-triangle queries
- Crease edge detection via dihedral angle threshold
- Crease snapping (point-to-edge projection)
- del-msh solver integration with self-intersection handling
- 13 unit tests, all passing

## What's Next

### `msh clean` command (bn-1er)

Single command that chains voxel + simplify:

```
msh clean input.glb -o output.obj --target-verts 2000 [--voxel-size 0.004]
```

Auto voxel size: bbox_max_dim / 250 (gives ~250^3 effective resolution).

### GLB output support (bn-3ck)

Detect `.obj` or `.glb` in output filename and write the correct format.
Currently all output is OBJ. GLB output would preserve materials/textures.

### Stage 3 improvements (bn-1ka, deferred)

Full Newton-CG solver using del-fem-core energy terms (StVK elastic + quadratic
bending) + del-ls sparse PCG solver. Only needed if we want to use coarser
voxel sizes for speed and rely on projection to recover detail.

## Dependencies

- `baby_shark 0.3.12` — CornerTable, VoxelRemesher, QEM decimation
- `del-msh-core 0.1.39` — collision-aware vertex movement, BVH, CCD (pure Rust)
- `nalgebra 0.33` — linear algebra

## References

- PaMO: Oh et al. 2025, arXiv 2509.05595
- QEM: Garland & Heckbert 1997
- del-* crates: nobuyuki83 (DelFEM2 author), pure Rust FEM/mesh library
