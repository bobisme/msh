//! Opt-in end-to-end contracts against the executable, not renderer mocks.
//! Run: cargo test --test automation -- --ignored --nocapture
#![cfg(feature = "remote")]
use jsonrpsee::{
    core::client::ClientT,
    http_client::{HttpClient, HttpClientBuilder},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "msh-automation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Service {
    child: Child,
    client: HttpClient,
    port: u16,
}
impl Service {
    fn start(root: &Scratch, native: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_msh"));
        if native {
            command.args([
                "view",
                "--remote",
                "--no-vsync",
                "--title",
                "msh native automation test",
            ]);
        } else {
            command.arg("serve");
        }
        let child = command
            .args(["--port", "0", "--size", "320x240", "--no-ground"])
            .stdout(Stdio::piped())
            .stderr(
                fs::File::create(root.file(if native { "native.log" } else { "serve.log" }))
                    .unwrap(),
            )
            .spawn()
            .unwrap();
        let mut child = child;
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(address) = line.strip_prefix("RPC_READY http://127.0.0.1:") {
                    let _ = send.send(address.parse::<u16>().unwrap());
                }
            }
        });
        let port = match receive.recv_timeout(Duration::from_secs(20)) {
            Ok(port) => port,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("No ready endpoint: {e}")
            }
        };
        let client = HttpClientBuilder::default()
            .request_timeout(Duration::from_secs(65))
            .build(format!("http://127.0.0.1:{port}"))
            .unwrap();
        Self {
            child,
            client,
            port,
        }
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        let mut arguments = jsonrpsee::core::params::ArrayParams::new();
        for value in params.as_array().unwrap() {
            arguments.insert(value).unwrap();
        }
        self.client.request(method, arguments).await.unwrap()
    }
    async fn fails(&self, method: &str, params: Value) {
        let mut arguments = jsonrpsee::core::params::ArrayParams::new();
        for value in params.as_array().unwrap() {
            arguments.insert(value).unwrap();
        }
        assert!(
            self.client
                .request::<Value, _>(method, arguments)
                .await
                .is_err()
        );
    }
    async fn capture(&self, path: &str) -> image::RgbaImage {
        let result = self.call("screenshot", json!([path])).await;
        let image = image::open(path).unwrap().to_rgba8();
        assert_eq!(
            (image.width(), image.height()),
            (
                result["width"].as_u64().unwrap() as u32,
                result["height"].as_u64().unwrap() as u32
            )
        );
        image
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn glb(path: &str, root: Value, mut blob: Vec<u8>) {
    let mut source = serde_json::to_vec(&root).unwrap();
    while source.len() % 4 != 0 {
        source.push(b' ');
    }
    while blob.len() % 4 != 0 {
        blob.push(0);
    }
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2u32.to_le_bytes());
    bytes.extend((28u32 + source.len() as u32 + blob.len() as u32).to_le_bytes());
    bytes.extend((source.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(source);
    bytes.extend((blob.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(blob);
    fs::write(path, bytes).unwrap();
}
fn target(path: &str, instances: bool) {
    let p: [f32; 9] = [-1., -1., 0., 1., -1., 0., 0., 1., 0.];
    let mut blob = bytemuck::cast_slice(&p).to_vec();
    blob.extend([0u8; 12]);
    let weights: [f32; 12] = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    blob.extend(bytemuck::cast_slice(&weights));
    let mut root = json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0,1]}],"nodes":[{"mesh":0,"skin":0},{"name":"root"}],"skins":[{"joints":[1]}],"buffers":[{"byteLength":blob.len()}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12},{"buffer":0,"byteOffset":48,"byteLength":48}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-1,-1,0],"max":[1,1,0]},{"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"},{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}],
        "meshes":[{"name":"triangle","primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}}]}]});
    if instances {
        root.as_object_mut().unwrap().remove("skins");
        root["meshes"][0]["primitives"][0]["attributes"] = json!({"POSITION":0});
        root["nodes"] = json!([{"mesh":0,"translation":[-2,0,0]},{"mesh":0,"translation":[2,0,0],"scale":[-1,1,1]}]);
    }
    glb(path, root, blob);
}
fn clips(path: &str, name: &str) {
    let values: [f32; 8] = [0., 1., 0., 0., 0., 0.6, 0., 0.];
    let blob = bytemuck::cast_slice(&values).to_vec();
    glb(
        path,
        json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"name":name}],"skins":[{"joints":[0]}],"buffers":[{"byteLength":blob.len()}],
        "bufferViews":[{"buffer":0,"byteLength":8},{"buffer":0,"byteOffset":8,"byteLength":24}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},{"bufferView":1,"componentType":5126,"count":2,"type":"VEC3"}],
        "animations":[{"name":"shift","samplers":[{"input":0,"output":1,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}]}]}),
        blob,
    );
}
async fn contract(native: bool) {
    let root = Scratch::new();
    let model = root.file("target.glb");
    target(&model, false);
    let scene = root.file("scene.glb");
    target(&scene, true);
    let source = root.file("clips.glb");
    clips(&source, "root");
    let bad = root.file("bad.glb");
    clips(&bad, "missing-root");
    let service = Service::start(&root, native);
    service.call("disable_ui", json!([])).await;
    let stats = service
        .call("load_model", json!([model, null, false, source]))
        .await;
    assert_eq!(stats["triangles"], 1);
    service
        .call("set_camera", json!([[0, 0, 4], [0, 0, 0]]))
        .await;
    service.call("set_animation", json!(["shift"])).await;
    let first = service.capture(&root.file("rest.png")).await;
    if !native {
        assert_eq!(first.dimensions(), (320, 240));
    }
    service.call("set_time", json!([0.5])).await;
    let pose = service.capture(&root.file("pose.png")).await;
    assert_ne!(first.as_raw(), pose.as_raw());
    service.call("set_frame", json!([15, 30])).await;
    assert_eq!(
        service.capture(&root.file("frame.png")).await.as_raw(),
        pose.as_raw()
    );
    service.fails("load_clips", json!([bad])).await;
    assert_eq!(
        service
            .call("list_animations", json!([]))
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    service.fails("set_time", json!([-1])).await;
    service
        .fails("load_model", json!([root.file("missing.glb")]))
        .await;
    assert_eq!(service.call("get_stats", json!([])).await["triangles"], 1);
    service.call("load_model", json!([scene, "all"])).await;
    let distance = 8.0 * (first.height() as f32 / first.width() as f32).max(1.0);
    service
        .call("set_camera", json!([[0, 0, distance], [0, 0, 0]]))
        .await;
    let stats = service.call("backface_stats", json!([])).await;
    assert!(stats["covered_px"].as_u64().unwrap() > 500);
    assert_eq!(stats["backface_px"], 0);
    service.call("enable_ui", json!([])).await;
    service.call("enable_ground", json!([])).await;
    assert_eq!(service.call("backface_stats", json!([])).await, stats);
    service.call("disable_ui", json!([])).await;
    service.call("disable_ground", json!([])).await;
    let blocked = root.file("blocked");
    fs::write(&blocked, b"file").unwrap();
    let shots = json!([[{"eye":[0,0,distance],"target":[0,0,0],"path":root.file("batch-a.png")},{"eye":[0,0,distance],"target":[0,0,0],"path":format!("{blocked}/bad.png")},{"eye":[0,0,-distance],"target":[0,0,0],"path":root.file("batch-b.png")}]]);
    let results = service.call("shoot", shots).await;
    assert_eq!(results.as_array().unwrap().len(), 3);
    assert!(results[0]["error"].is_null());
    assert!(results[1]["error"].is_string());
    assert!(results[2]["error"].is_null());
    service.fails("shoot", json!([[]])).await;
    let mut pending = Vec::new();
    for i in 0..8 {
        let client = service.client.clone();
        let path = root.file(&format!("concurrent-{i}.png"));
        pending.push(tokio::spawn(async move {
            let result: Value = client.request("screenshot", (path.clone(),)).await.unwrap();
            assert!(Path::new(&path).is_file());
            result
        }));
    }
    for job in pending {
        job.await.unwrap();
    }
    service
        .call("set_camera", json!([[0, 4, 0], [0, 0, 0]]))
        .await;
    service.capture(&root.file("top.png")).await;
    service.call("set_clear_color", json!([1, 0, 0, 1])).await;
    assert_eq!(
        service
            .capture(&root.file("red.png"))
            .await
            .get_pixel(0, 0)
            .0,
        [255, 0, 0, 255]
    );
    let collision = Command::new(env!("CARGO_BIN_EXE_msh"))
        .args([
            "serve",
            "--port",
            &service.port.to_string(),
            "--size",
            "32x32",
        ])
        .output()
        .unwrap();
    assert!(!collision.status.success());
    if !native {
        let other = Service::start(&root, false);
        assert_ne!(service.port, other.port);
        let output = root.file("atlas.png");
        assert!(
            Command::new(env!("CARGO_BIN_EXE_msh"))
                .args([
                    "render",
                    &scene,
                    "--mesh",
                    "all",
                    "--angles",
                    "8",
                    "--sprite-sheet",
                    "--width",
                    "64",
                    "--height",
                    "64",
                    "--transparent-bg",
                    "--out",
                    &output
                ])
                .status()
                .unwrap()
                .success()
        );
        let atlas = image::open(output).unwrap().to_rgba8();
        assert_eq!(atlas.dimensions(), (64, 512));
        assert_eq!(atlas.get_pixel(0, 0).0[3], 0);
        if let Ok(tool) = std::env::var("GLTFPACK") {
            let packed = root.file("packed.glb");
            assert!(
                Command::new(tool)
                    .args(["-i", &scene, "-o", &packed, "-cc", "-kn", "-km"])
                    .status()
                    .unwrap()
                    .success()
            );
            service.call("load_model", json!([packed, "all"])).await;
        }
    }
}
#[test]
#[ignore = "Requires a Vulkan adapter; runs a windowless executable"]
fn offscreen_automation_contract() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(contract(false));
}
#[test]
#[ignore = "Requires a desktop and Vulkan; opens a labeled window and closes it"]
fn native_automation_contract() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(contract(true));
}
