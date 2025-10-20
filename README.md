# msh - 3D Mesh CLI Tool

A command-line tool for 3D mesh analysis, repair, and visualization. Supports
`.obj` and `.glb` file formats.

## Features

- **Mesh Remeshing**: Fix and incrementally remesh your 3D models with multiple
  algorithms
- **3D Viewer**: Interactive viewer for inspecting meshes
- **Mesh Analysis**: Get detailed statistics about your mesh
- **Manifold Checking**: Verify if your mesh is watertight
- **Automatic Repair**: Fix holes and issues in damaged meshes

![Mesh Viewer](shot-1.png)

## Installation

```bash
cargo install mshmash
```

## Usage

### View a Mesh

Interactively view a mesh file in a 3D viewer:

```bash
msh view <INPUT>
msh view model.obj
msh view scene.glb --mesh name  # For GLB files with multiple meshes
```

**Options:**

- `-m, --mesh <MESH>`: Mesh name (required if GLB contains multiple meshes)

### Remesh a Mesh

Remesh a file with automatic fixing and incremental remeshing:

```bash
msh remesh <INPUT> --out <OUTPUT>
msh remesh model.obj --out fixed.obj
msh remesh scene.glb --out output.obj --mesh meshname
```

**Options:**

- `-o, --out <OUT>`: Output mesh file (.obj) - **required**
- `-m, --mesh <MESH>`: Mesh name (required if GLB contains multiple meshes)
- `-i, --iterations <ITERATIONS>`: Number of incremental remeshing iterations
  (default: 10)
- `-t, --target-edge-length <TARGET_EDGE_LENGTH>`: Target edge length for
  incremental remeshing (default: 0.01)
- `-v, --voxel-size <VOXEL_SIZE>`: Voxel size for fix step (default: 0.01)
- `--tolerance <TOLERANCE>`: Vertex merge tolerance for fix step (default:
  0.0001)
- `--no-fix`: Skip the fix step (just do incremental remesh)

**Remeshing Methods:**

- **Incremental** (default): Edge-based operations that progressively improve
  mesh quality

  ```bash
  msh remesh model.obj --out output.obj incremental
  ```

- **Voxel**: Converts mesh to signed distance field (SDF) then remeshes for
  clean topology

  ```bash
  msh remesh model.obj --out output.obj voxel
  ```

### Get Mesh Statistics

Display detailed information about a mesh:

```bash
msh stats <INPUT>
msh stats model.obj
msh stats scene.glb --mesh meshname
```

**Options:**

- `-m, --mesh <MESH>`: Mesh name (required if GLB contains multiple meshes)

### Check if Mesh is Manifold

Verify if a mesh is manifold (watertight):

```bash
msh check <INPUT>
msh check model.obj
```

**Options:**

- `-m, --mesh <MESH>`: Mesh name (required if GLB contains multiple meshes)

### Fix Mesh Holes

Automatically fix holes and issues in a damaged mesh:

```bash
msh fix <INPUT> --out <OUTPUT>
msh fix damaged.obj --out fixed.obj
```

**Options:**

- `-o, --out <OUT>`: Output mesh file (.obj) - **required**
- `-m, --mesh <MESH>`: Mesh name (required if GLB contains multiple meshes)
- `-v, --voxel-size <VOXEL_SIZE>`: Voxel size for remeshing (default: 0.01)
- `-t, --tolerance <TOLERANCE>`: Merge vertices closer than this distance before
  fixing (default: 0.0001)
- `--no-merge`: Skip vertex merging step

### Inspect GLB/glTF Files

Display the structure and contents of a GLB or glTF file:

```bash
msh inspect-glb <INPUT>
msh inspect-glb scene.glb
msh inspect-glb model.glb --json  # Output as JSON
```

This command shows the scene hierarchy, including:

- Scene structure and node tree
- Transform data (position, rotation, scale)
- Mesh information (name, primitive count)
- Custom properties (extras)
- Cameras and other components

**Options:**

- `--json`: Output as JSON instead of tree format

## Supported Formats

- **Input:** `.obj`, `.glb`
- **Output:** `.obj`

## Common Workflows

**Repair a damaged mesh:**

```bash
msh fix broken.obj --out repaired.obj
```

**High-quality remesh with custom edge length:**

```bash
msh remesh model.obj --out output.obj --target-edge-length 0.005 --iterations 20
```

**Voxel-based remeshing for topology cleanup:**

```bash
msh remesh model.obj --out clean.obj voxel --voxel-size 0.02
```

**Inspect mesh before and after:**

```bash
msh view original.obj
msh remesh original.obj --out fixed.obj
msh view fixed.obj
```

## Remote Control & RenderDoc Features

### Remote Control (Feature: `remote`)

Control the mesh viewer remotely via JSON-RPC. Perfect for automated workflows, scripting, or external tool integration.

#### Enable Remote Control

Start the viewer with remote control enabled:

```bash
# Build with remote feature
cargo build --features remote

# Start viewer with RPC server
msh view model.obj --remote
```

The viewer will start a JSON-RPC server on `http://127.0.0.1:9001` and display:
```
✓ RPC server ready at http://127.0.0.1:9001
  Available methods:
    - load_model(path, mesh_name?)
    - set_rotation(x, y, z)
    - rotate_around_axis(axis, angle)
    ...
```

#### Remote Commands

Control the running viewer from another terminal:

**Load a Model**
```bash
msh remote load path/to/model.obj
msh remote load scene.glb --mesh "Body"
```

**Rotate the Model**
```bash
# Set absolute rotation (Euler angles in radians)
msh remote rotate 0 1.57 0

# Rotate around axis with angle notation
msh remote rotate-axis 0,1,0 90d    # 90 degrees around Y axis
msh remote rotate-axis 1,0,0 1.57r  # 1.57 radians around X axis
```

**Control Camera**
```bash
# Set camera position
msh remote camera-pos 5.0 3.0 5.0

# Set camera target (look-at point)
msh remote camera-target 0 0 0
```

**Toggle Display Options**
```bash
# Wireframe
msh remote enable-wireframe
msh remote disable-wireframe
msh remote toggle-wireframe

# Backface visualization (shows red reversed faces)
msh remote enable-backfaces
msh remote disable-backfaces
msh remote toggle-backfaces

# UI overlay
msh remote enable-ui
msh remote disable-ui
msh remote toggle-ui
```

**Get Mesh Statistics**
```bash
msh remote stats
```
Output:
```
=== Mesh Statistics ===
Vertices:  1234
Edges:     3702
Faces:     2468
Manifold:  Yes
```

**Take Screenshot**
```bash
msh remote screenshot output.png
msh remote screenshot captures/view.png    # Creates 'captures' dir if needed
msh remote screenshot deep/nested/dirs/shot.png  # Creates all parent dirs
```

**Capture Frame (RenderDoc)**
```bash
msh remote capture
msh remote capture /path/to/save.rdc
```

**Quit the Viewer**
```bash
msh remote quit
```

### RenderDoc Integration (Feature: `renderdoc`)

Capture GPU frames for debugging and analysis using RenderDoc.

#### Build with RenderDoc Support

```bash
cargo build --features renderdoc
# or with both features
cargo build --features remote,renderdoc
```

#### Capture Frames

**Launch with RenderDoc:**
```bash
# Build with RenderDoc support
cargo build --features renderdoc --release

# Launch through RenderDoc
renderdoccmd ./target/release/msh view model.obj
# or
qrenderdoc  # Then use GUI: File -> Launch Application
```

When launched through RenderDoc, F12 will capture frames (injected by RenderDoc).

**Via Remote Control:**
```bash
# Terminal 1: Launch with both features
cargo build --features remote,renderdoc --release
renderdoccmd ./target/release/msh view model.obj --remote

# Terminal 2: Trigger capture remotely
msh remote capture                               # Default RenderDoc location
msh remote capture "captures/my_mesh"            # Relative to your current directory
msh remote capture "/tmp/debug/mesh_analysis"    # Absolute path
```

The path parameter sets RenderDoc's capture file path template:
- **Relative paths** are resolved from your current working directory (where you run `msh remote`)
- **Absolute paths** are used as-is
- RenderDoc appends a timestamp and `.rdc` extension to the final file

**Note:** RenderDoc works by injecting itself into your application process. The app must be launched through RenderDoc (via `renderdoccmd` or the RenderDoc GUI) for frame capture to work.

### Angle Notation

The remote control system supports flexible angle notation:

- **Degrees**: `90d`, `180d`, `45d`, `-90d`
- **Radians**: `1.57r`, `3.14r`, `0.785r`
- **No unit** (interpreted as radians): `1.57`, `3.14`

Examples:
```bash
msh remote rotate-axis 0,1,0 90d      # Quarter turn
msh remote rotate-axis 1,0,0 180d    # Half turn
msh remote rotate-axis 0,0,1 1.57r   # ~90° in radians
```

### Example Workflow: Automated Inspection

```bash
# Terminal 1: Start viewer with remote control
msh view model.obj --remote

# Terminal 2: Automate inspection with screenshots
msh remote rotate 0 0 0                     # Reset rotation
msh remote camera-pos 10 5 10               # Position camera
msh remote enable-wireframe                 # Show wireframe
msh remote rotate-axis 0,1,0 45d           # Rotate 45° around Y
msh remote screenshot "shots/front_view.png"  # Take screenshot

msh remote rotate-axis 0,1,0 90d           # Rotate another 90°
msh remote screenshot "shots/side_view.png"   # Another screenshot

msh remote disable-wireframe                # Toggle for comparison
msh remote screenshot "shots/side_shaded.png"

msh remote stats                            # Get mesh info
```

**With RenderDoc for GPU debugging:**
```bash
# Terminal 1: Start with RenderDoc
renderdoccmd msh view model.obj --remote

# Terminal 2: Mix screenshots and RenderDoc captures
msh remote screenshot "analysis/mesh.png"        # High-level view (PNG)
msh remote capture "analysis/mesh"               # GPU-level capture (.rdc)
```

**Note:**
- Launch with `renderdoccmd` to enable frame capture. Without it, `msh remote capture` will report that RenderDoc is not available.
- Relative paths are resolved from your current directory, so if you're in `/home/user/projects/models/`, the path `"captures/front_view.png"` becomes `/home/user/projects/models/captures/front_view.png`.
- Parent directories are automatically created for screenshots, so `"output/shots/view.png"` will create the `output/shots/` directory structure if it doesn't exist.

### JSON-RPC API

The RPC server implements JSON-RPC 2.0 over HTTP. You can also call methods directly:

```bash
curl -X POST http://127.0.0.1:9001 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"set_rotation","params":[0,1.57,0],"id":1}'
```

Available methods:
- `load_model(path: String, mesh_name: Option<String>)`
- `set_rotation(x: f32, y: f32, z: f32)`
- `rotate_around_axis(axis: Vec<f32>, angle: String)`
- `set_camera_position(x: f32, y: f32, z: f32)`
- `set_camera_target(x: f32, y: f32, z: f32)`
- `enable_wireframe()` / `disable_wireframe()` / `toggle_wireframe()`
- `enable_backfaces()` / `disable_backfaces()` / `toggle_backfaces()`
- `enable_ui()` / `disable_ui()` / `toggle_ui()`
- `get_stats()` → `{vertices, edges, faces, is_manifold, holes}`
- `screenshot(path: String)` - Save current view as PNG
- `capture_frame(path: Option<String>)` (requires `renderdoc` feature)
- `quit()` - Exit the viewer

## Feature Flags

- **`remote`**: Enables JSON-RPC server and remote control CLI
  - Dependencies: jsonrpsee, tokio, crossbeam
  - Build: `cargo build --features remote`

- **`renderdoc`**: Enables RenderDoc frame capture
  - Dependencies: renderdoc crate
  - Build: `cargo build --features renderdoc`

- **Both**: `cargo build --features remote,renderdoc`
