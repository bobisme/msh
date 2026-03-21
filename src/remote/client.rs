#[cfg(feature = "remote")]
use jsonrpsee::core::client::ClientT;
#[cfg(feature = "remote")]
use jsonrpsee::core::params::ArrayParams;
#[cfg(feature = "remote")]
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};

#[cfg(feature = "remote")]
use crate::rpc::types::MeshStatsResponse;

#[cfg(feature = "remote")]
pub async fn create_client(url: &str) -> Result<HttpClient, Box<dyn std::error::Error>> {
    let client = HttpClientBuilder::default()
        .build(url)?;
    Ok(client)
}

#[cfg(feature = "remote")]
pub async fn load_model(
    client: &HttpClient,
    path: String,
    mesh_name: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("load_model", (path, mesh_name))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_rotation(
    client: &HttpClient,
    x: f32,
    y: f32,
    z: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_rotation", (x, y, z))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn rotate_around_axis(
    client: &HttpClient,
    axis: Vec<f32>,
    angle: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("rotate_around_axis", (axis, angle))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_camera_position(
    client: &HttpClient,
    x: f32,
    y: f32,
    z: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_camera_position", (x, y, z))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_camera_target(
    client: &HttpClient,
    x: f32,
    y: f32,
    z: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_camera_target", (x, y, z))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn enable_wireframe(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("enable_wireframe", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn disable_wireframe(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("disable_wireframe", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn toggle_wireframe(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("toggle_wireframe", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn enable_backfaces(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("enable_backfaces", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn disable_backfaces(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("disable_backfaces", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn toggle_backfaces(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("toggle_backfaces", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn enable_ui(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("enable_ui", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn disable_ui(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("disable_ui", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn toggle_ui(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("toggle_ui", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn get_stats(client: &HttpClient) -> Result<MeshStatsResponse, Box<dyn std::error::Error>> {
    let response: MeshStatsResponse = client
        .request("get_stats", ArrayParams::new())
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn capture_frame(
    client: &HttpClient,
    path: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("capture_frame", (path,))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn screenshot(
    client: &HttpClient,
    path: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("screenshot", (path,))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_projection(
    client: &HttpClient,
    mode: String,
    value: Option<f32>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_projection", (mode, value))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_clear_color(
    client: &HttpClient,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_clear_color", (r, g, b, a))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_shading(
    client: &HttpClient,
    mode: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_shading", (mode,))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_base_color(
    client: &HttpClient,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_base_color", (r, g, b, a))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn set_light_direction(
    client: &HttpClient,
    x: f32,
    y: f32,
    z: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("set_light_direction", (x, y, z))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn apply_preset(
    client: &HttpClient,
    name: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("apply_preset", (name,))
        .await?;
    Ok(response)
}

#[cfg(feature = "remote")]
pub async fn quit(client: &HttpClient) -> Result<String, Box<dyn std::error::Error>> {
    let response: String = client
        .request("quit", ArrayParams::new())
        .await?;
    Ok(response)
}
