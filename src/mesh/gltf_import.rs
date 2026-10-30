//! Decode meshopt buffer views and normalize quantized attributes before handing
//! validated glTF to the common loader. Encoded and decoded retention share a budget.
use base64::Engine;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_BYTES: usize = 512 * 1024 * 1024;
const MAX_JSON: usize = 64 * 1024 * 1024;

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err("glTF source exceeds 512 MiB".into());
    }
    Ok(bytes)
}
fn number(value: &Value, key: &str) -> Result<usize> {
    Ok(usize::try_from(
        value
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("Missing/invalid {key}"))?,
    )?)
}
fn add_buffer(
    root: &mut Value,
    buffers: &mut Vec<gltf::buffer::Data>,
    bytes: Vec<u8>,
    budget: &mut usize,
) -> Result<usize> {
    *budget = budget
        .checked_add(bytes.len())
        .ok_or("Decoded buffer budget overflow")?;
    if *budget > MAX_BYTES {
        return Err("glTF buffers exceed 512 MiB encoded plus decoded".into());
    }
    let index = buffers.len();
    root["buffers"]
        .as_array_mut()
        .ok_or("Missing buffers")?
        .push(json!({"byteLength":bytes.len()}));
    buffers.push(gltf::buffer::Data(bytes));
    Ok(index)
}
fn view_bytes<'a>(
    root: &Value,
    buffers: &'a [gltf::buffer::Data],
    index: usize,
) -> Result<&'a [u8]> {
    let view = root
        .get("bufferViews")
        .and_then(|v| v.get(index))
        .ok_or("Invalid bufferView")?;
    let buffer = buffers
        .get(number(view, "buffer")?)
        .ok_or("Invalid buffer")?;
    let start = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let end = start
        .checked_add(number(view, "byteLength")?)
        .ok_or("bufferView range overflow")?;
    buffer
        .get(start..end)
        .ok_or_else(|| "bufferView exceeds source buffer".into())
}

fn decode_views(
    root: &mut Value,
    buffers: &mut Vec<gltf::buffer::Data>,
    budget: &mut usize,
) -> Result<()> {
    let count = root
        .get("bufferViews")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    for index in 0..count {
        let Some(extension) = root["bufferViews"][index]["extensions"]["EXT_meshopt_compression"]
            .as_object()
            .map(|e| Value::Object(e.clone()))
        else {
            continue;
        };
        let elements = number(&extension, "count")?;
        let stride = number(&extension, "byteStride")?;
        let length = elements
            .checked_mul(stride)
            .ok_or("meshopt decoded size overflow")?;
        if elements == 0
            || length > MAX_BYTES.saturating_sub(*budget)
            || length != number(&root["bufferViews"][index], "byteLength")?
        {
            return Err("meshopt decoded length invalid or exceeds remaining buffer budget".into());
        }
        if let Some(parent_stride) = root["bufferViews"][index].get("byteStride")
            && parent_stride.as_u64() != Some(stride as u64)
        {
            return Err("meshopt stride differs from parent bufferView".into());
        }
        let mode = extension["mode"].as_str().ok_or("Missing meshopt mode")?;
        let filter = extension
            .get("filter")
            .and_then(Value::as_str)
            .unwrap_or("NONE");
        match mode {
            "ATTRIBUTES" if stride > 0 && stride <= 256 && stride % 4 == 0 => {}
            "TRIANGLES"
                if (stride == 2 || stride == 4) && elements % 3 == 0 && filter == "NONE" => {}
            "INDICES" if (stride == 2 || stride == 4) && filter == "NONE" => {}
            _ => return Err("Invalid meshopt mode/stride/count/filter".into()),
        }
        match filter {
            "NONE" => {}
            "OCTAHEDRAL" if stride == 4 || stride == 8 => {}
            "QUATERNION" if stride == 8 => {}
            "EXPONENTIAL" if stride % 4 == 0 => {}
            _ => return Err("Invalid meshopt filter/stride".into()),
        }
        let source = buffers
            .get(number(&extension, "buffer")?)
            .ok_or("Invalid meshopt source buffer")?;
        let start = extension
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let end = start
            .checked_add(number(&extension, "byteLength")?)
            .ok_or("meshopt source range overflow")?;
        let encoded = source
            .get(start..end)
            .ok_or("meshopt source range exceeds buffer")?;
        // Four-byte alignment satisfies all decoder/filter integer access requirements.
        let mut words = vec![0u32; length.div_ceil(4)];
        // SAFETY: all codec preconditions are checked above. The destination owns at
        // least count*stride bytes; source is a checked slice; decoders validate streams.
        let status = unsafe {
            let destination = words.as_mut_ptr().cast();
            match mode {
                "ATTRIBUTES" => meshopt::ffi::meshopt_decodeVertexBuffer(
                    destination,
                    elements,
                    stride,
                    encoded.as_ptr(),
                    encoded.len(),
                ),
                "TRIANGLES" => meshopt::ffi::meshopt_decodeIndexBuffer(
                    destination,
                    elements,
                    stride,
                    encoded.as_ptr(),
                    encoded.len(),
                ),
                _ => meshopt::ffi::meshopt_decodeIndexSequence(
                    destination,
                    elements,
                    stride,
                    encoded.as_ptr(),
                    encoded.len(),
                ),
            }
        };
        if status != 0 {
            return Err(format!("meshopt decode failed ({status})").into());
        }
        // SAFETY: filters operate in place with the same checked length/alignment;
        // their stride restrictions were validated before decoder entry.
        unsafe {
            let destination = words.as_mut_ptr().cast();
            match filter {
                "OCTAHEDRAL" => {
                    meshopt::ffi::meshopt_decodeFilterOct(destination, elements, stride)
                }
                "QUATERNION" => {
                    meshopt::ffi::meshopt_decodeFilterQuat(destination, elements, stride)
                }
                "EXPONENTIAL" => {
                    meshopt::ffi::meshopt_decodeFilterExp(destination, elements, stride)
                }
                _ => {}
            }
        }
        let decoded = bytemuck::cast_slice(&words)[..length].to_vec();
        let buffer = add_buffer(root, buffers, decoded, budget)?;
        let view = &mut root["bufferViews"][index];
        view["buffer"] = json!(buffer);
        view["byteOffset"] = json!(0);
        view["extensions"]
            .as_object_mut()
            .unwrap()
            .remove("EXT_meshopt_compression");
    }
    Ok(())
}

fn component(bytes: &[u8], kind: usize, normalized: bool) -> Result<f32> {
    Ok(match kind {
        5120 => {
            let v = bytes[0] as i8 as f32;
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = bytes[0] as f32;
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let v = i16::from_le_bytes(bytes[..2].try_into()?) as f32;
            if normalized {
                (v / 32767.0).max(-1.0)
            } else {
                v
            }
        }
        5123 => {
            let v = u16::from_le_bytes(bytes[..2].try_into()?) as f32;
            if normalized { v / 65535.0 } else { v }
        }
        _ => return Err("Unsupported quantized component type".into()),
    })
}
fn quantized_attributes(
    root: &mut Value,
    buffers: &mut Vec<gltf::buffer::Data>,
    budget: &mut usize,
) -> Result<()> {
    let mut targets = std::collections::BTreeSet::new();
    if let Some(meshes) = root["meshes"].as_array() {
        for mesh in meshes {
            if let Some(primitives) = mesh["primitives"].as_array() {
                for primitive in primitives {
                    if let Some(attributes) = primitive["attributes"].as_object() {
                        for (name, index) in attributes {
                            if name == "POSITION"
                                || name == "NORMAL"
                                || name == "TANGENT"
                                || name.starts_with("TEXCOORD_")
                            {
                                targets
                                    .insert(index.as_u64().ok_or("Invalid attribute accessor")?
                                        as usize);
                            }
                        }
                    }
                }
            }
        }
    }
    for mesh in root["meshes"].as_array().into_iter().flatten() {
        for primitive in mesh["primitives"].as_array().into_iter().flatten() {
            for target in primitive["targets"].as_array().into_iter().flatten() {
                if let Some(attributes) = target.as_object() {
                    for index in attributes.values() {
                        targets.insert(
                            index.as_u64().ok_or("Invalid morph attribute accessor")? as usize
                        );
                    }
                }
            }
        }
    }
    for index in targets {
        let accessor = root["accessors"]
            .get(index)
            .ok_or("Invalid attribute accessor")?
            .clone();
        let kind = number(&accessor, "componentType")?;
        if kind == 5126 {
            continue;
        }
        let components = match accessor["type"].as_str() {
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            _ => return Err("Invalid quantized attribute dimensions".into()),
        };
        let size = match kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => return Err("Invalid quantized attribute type".into()),
        };
        let count = number(&accessor, "count")?;
        let length = count
            .checked_mul(components)
            .and_then(|n| n.checked_mul(4))
            .ok_or("Quantized accessor size overflow")?;
        if count == 0 || length > MAX_BYTES.saturating_sub(*budget) {
            return Err("Quantized attribute exceeds decoded buffer budget".into());
        }
        let normalized = accessor
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut values = vec![0f32; count * components];
        if let Some(view) = accessor.get("bufferView").and_then(Value::as_u64) {
            let bytes = view_bytes(root, buffers, view as usize)?;
            let stride = root["bufferViews"][view as usize]
                .get("byteStride")
                .and_then(Value::as_u64)
                .unwrap_or((size * components) as u64) as usize;
            let offset = accessor
                .get("byteOffset")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let end = (count - 1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(offset))
                .and_then(|n| n.checked_add(size * components))
                .ok_or("Quantized accessor span overflow")?;
            if stride < size * components || end > bytes.len() {
                return Err("Quantized accessor exceeds bufferView".into());
            }
            for i in 0..count {
                for c in 0..components {
                    let start = offset + i * stride + c * size;
                    values[i * components + c] =
                        component(&bytes[start..start + size], kind, normalized)?;
                }
            }
        }
        if let Some(sparse) = accessor.get("sparse") {
            let entries = number(sparse, "count")?;
            if entries > count {
                return Err("Sparse accessor count exceeds attribute count".into());
            }
            let indices = &sparse["indices"];
            let index_kind = number(indices, "componentType")?;
            let index_size = match index_kind {
                5121 => 1,
                5123 => 2,
                5125 => 4,
                _ => return Err("Invalid sparse index type".into()),
            };
            let ib = view_bytes(root, buffers, number(indices, "bufferView")?)?;
            let vb = view_bytes(root, buffers, number(&sparse["values"], "bufferView")?)?;
            let io = indices
                .get("byteOffset")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let vo = sparse["values"]
                .get("byteOffset")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let index_end = entries
                .checked_mul(index_size)
                .and_then(|n| n.checked_add(io))
                .ok_or("Sparse index span overflow")?;
            let value_end = entries
                .checked_mul(size * components)
                .and_then(|n| n.checked_add(vo))
                .ok_or("Sparse value span overflow")?;
            if index_end > ib.len() || value_end > vb.len() {
                return Err("Sparse accessor exceeds bufferView".into());
            }
            let mut previous = None;
            for i in 0..entries {
                let start = io + i * index_size;
                let vertex = match index_kind {
                    5121 => ib[start] as usize,
                    5123 => u16::from_le_bytes(ib[start..start + 2].try_into()?) as usize,
                    _ => u32::from_le_bytes(ib[start..start + 4].try_into()?) as usize,
                };
                if vertex >= count || previous.is_some_and(|p| vertex <= p) {
                    return Err("Sparse indices must be sorted, unique and within accessor".into());
                }
                previous = Some(vertex);
                for c in 0..components {
                    let start = vo + (i * components + c) * size;
                    values[vertex * components + c] =
                        component(&vb[start..start + size], kind, normalized)?;
                }
            }
        }
        let mut min = vec![f32::INFINITY; components];
        let mut max = vec![f32::NEG_INFINITY; components];
        for row in values.chunks_exact(components) {
            for c in 0..components {
                min[c] = min[c].min(row[c]);
                max[c] = max[c].max(row[c]);
            }
        }
        let buffer = add_buffer(
            root,
            buffers,
            bytemuck::cast_slice(&values).to_vec(),
            budget,
        )?;
        let view = root["bufferViews"]
            .as_array()
            .ok_or("Missing bufferViews")?
            .len();
        root["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":buffer,"byteLength":length}));
        let target = &mut root["accessors"][index];
        target["bufferView"] = json!(view);
        target["byteOffset"] = json!(0);
        target["componentType"] = json!(5126);
        target["min"] = json!(min);
        target["max"] = json!(max);
        target.as_object_mut().unwrap().remove("normalized");
        target.as_object_mut().unwrap().remove("sparse");
    }
    Ok(())
}

fn validate_accessors(document: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Result<()> {
    let mut total = 0usize;
    let span =
        |offset: usize, count: usize, stride: usize, size: usize, length: usize| -> Result<()> {
            let end = count
                .checked_sub(1)
                .and_then(|n| n.checked_mul(stride))
                .and_then(|n| n.checked_add(offset))
                .and_then(|n| n.checked_add(size))
                .ok_or("Accessor span overflow or zero count")?;
            if stride < size || end > length {
                return Err("Accessor exceeds bufferView".into());
            }
            Ok(())
        };
    for accessor in document.accessors() {
        let size = accessor.size();
        let count = accessor.count();
        total = total
            .checked_add(count.checked_mul(size).ok_or("Accessor size overflow")?)
            .ok_or("Accessor budget overflow")?;
        if count == 0 || total > MAX_BYTES {
            return Err("Expanded accessors exceed 512 MiB budget or have zero count".into());
        }
        if let Some(view) = accessor.view() {
            span(
                accessor.offset(),
                count,
                view.stride().unwrap_or(size),
                size,
                view.length(),
            )?;
        }
        if let Some(sparse) = accessor.sparse() {
            if sparse.count() == 0 || sparse.count() > count {
                return Err("Invalid sparse count".into());
            }
            let indices = sparse.indices();
            let iv = indices.view();
            let is = indices.index_type().size();
            let stride = iv.stride().unwrap_or(is);
            span(indices.offset(), sparse.count(), stride, is, iv.length())?;
            let values = sparse.values();
            let vv = values.view();
            span(
                values.offset(),
                sparse.count(),
                vv.stride().unwrap_or(size),
                size,
                vv.length(),
            )?;
            let data = &buffers[iv.buffer().index()];
            let start = iv.offset() + indices.offset();
            let mut previous = None;
            for i in 0..sparse.count() {
                let offset = start + i * stride;
                let index = match is {
                    1 => data[offset] as usize,
                    2 => u16::from_le_bytes(data[offset..offset + 2].try_into()?) as usize,
                    _ => u32::from_le_bytes(data[offset..offset + 4].try_into()?) as usize,
                };
                if index >= count || previous.is_some_and(|p| index <= p) {
                    return Err("Sparse indices must be ordered, unique and in range".into());
                }
                previous = Some(index);
            }
        }
    }
    Ok(())
}

pub fn import(
    path: &Path,
) -> Result<(
    gltf::Document,
    Vec<gltf::buffer::Data>,
    Vec<gltf::image::Data>,
)> {
    let bytes = read_bounded(path)?;
    let json_length = if bytes.starts_with(b"glTF") && bytes.len() >= 20 {
        u32::from_le_bytes(bytes[12..16].try_into()?) as usize
    } else {
        bytes.len()
    };
    if json_length > MAX_JSON {
        return Err("glTF JSON exceeds 64 MiB".into());
    }
    let source = gltf::Gltf::from_slice_without_validation(&bytes)?;
    let mut root = serde_json::to_value(source.document.as_json())?;
    if root.get("buffers").is_none() {
        root["buffers"] = json!([]);
    }
    if root.get("bufferViews").is_none() {
        root["bufferViews"] = json!([]);
    }
    let mut buffers = Vec::new();
    let mut budget = 0usize;
    let mut blob = source.blob;
    for (index, buffer) in root["buffers"].as_array().unwrap().iter().enumerate() {
        if number(buffer, "byteLength")? > MAX_BYTES {
            return Err("glTF buffer declares more than 512 MiB".into());
        }
        let data = if let Some(uri) = buffer.get("uri").and_then(Value::as_str) {
            if uri.starts_with("data:") {
                let encoded = uri
                    .split_once(";base64,")
                    .ok_or("Only base64 buffer data URIs are supported")?
                    .1;
                if encoded.len() > MAX_BYTES / 3 * 4 + 4 {
                    return Err("Embedded buffer exceeds budget".into());
                }
                base64::engine::general_purpose::STANDARD.decode(encoded)?
            } else {
                let uri = uri
                    .strip_prefix("file://")
                    .or_else(|| uri.strip_prefix("file:"))
                    .unwrap_or(uri);
                if uri.contains("://") {
                    return Err("Network glTF buffer URIs are unsupported".into());
                }
                let decoded = urlencoding::decode(uri)?;
                read_bounded(
                    &path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(decoded.as_ref()),
                )?
            }
        } else if index == 0 {
            blob.take().unwrap_or_default()
        } else {
            Vec::new()
        };
        budget = budget
            .checked_add(data.len())
            .ok_or("Source buffer budget overflow")?;
        if budget > MAX_BYTES {
            return Err("glTF source buffers exceed 512 MiB".into());
        }
        buffers.push(gltf::buffer::Data(data));
    }
    decode_views(&mut root, &mut buffers, &mut budget)?;
    let quantized = root
        .get("extensionsUsed")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|v| v == "KHR_mesh_quantization"))
        || root
            .get("extensionsRequired")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|v| v == "KHR_mesh_quantization"));
    if quantized {
        quantized_attributes(&mut root, &mut buffers, &mut budget)?;
    }
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(extensions) = root.get_mut(key).and_then(Value::as_array_mut) {
            extensions.retain(|v| v != "EXT_meshopt_compression" && v != "KHR_mesh_quantization");
        }
    }
    // Only decoded extensions are removed. Every other required extension is still validated.
    let document = gltf::Document::from_json(serde_json::from_value(root)?)?;
    for view in document.views() {
        let buffer = buffers
            .get(view.buffer().index())
            .ok_or("Invalid buffer reference")?;
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > buffer.len())
        {
            return Err("bufferView exceeds decoded buffer".into());
        }
    }
    validate_accessors(&document, &buffers)?;
    let images = gltf::import_images(
        &document,
        Some(path.parent().unwrap_or(Path::new("."))),
        &buffers,
    )?;
    Ok((document, buffers, images))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compressed_fixture(name: &str, corrupt: bool, huge: bool) -> std::path::PathBuf {
        let positions: [[i16; 4]; 3] = [[0, 0, 0, 0], [100, 0, 0, 0], [0, 100, 0, 0]];
        let mut vertices = meshopt::encode_vertex_buffer(&positions).unwrap();
        if corrupt {
            vertices[0] = 0;
        }
        let indices = meshopt::encode_index_buffer(&[0, 1, 2], 3).unwrap();
        let index_offset = vertices.len();
        let mut blob = vertices.clone();
        blob.extend(&indices);
        let decoded = if huge { MAX_BYTES as u64 + 1 } else { 24 };
        let root = json!({
            "asset":{"version":"2.0"}, "extensionsRequired":["EXT_meshopt_compression","KHR_mesh_quantization"],
            "extensionsUsed":["EXT_meshopt_compression","KHR_mesh_quantization"],
            "scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0,"scale":[0.01,0.01,0.01]}],
            "buffers":[{"byteLength":blob.len()},{"byteLength":decoded+6,"extensions":{"EXT_meshopt_compression":{"fallback":true}}}],
            "bufferViews":[
                {"buffer":1,"byteLength":decoded,"byteStride":8,"extensions":{"EXT_meshopt_compression":{"buffer":0,"byteOffset":0,"byteLength":vertices.len(),"byteStride":8,"count":3,"mode":"ATTRIBUTES"}}},
                {"buffer":1,"byteOffset":24,"byteLength":6,"extensions":{"EXT_meshopt_compression":{"buffer":0,"byteOffset":index_offset,"byteLength":indices.len(),"byteStride":2,"count":3,"mode":"TRIANGLES"}}}
            ],
            "accessors":[{"bufferView":0,"componentType":5122,"count":3,"type":"VEC3","min":[0,0,0],"max":[100,100,0]},
                {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}]
        });
        let mut json = serde_json::to_vec(&root).unwrap();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        while blob.len() % 4 != 0 {
            blob.push(0);
        }
        let mut glb = Vec::new();
        glb.extend(b"glTF");
        glb.extend(2u32.to_le_bytes());
        glb.extend((28u32 + json.len() as u32 + blob.len() as u32).to_le_bytes());
        glb.extend((json.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(json);
        glb.extend((blob.len() as u32).to_le_bytes());
        glb.extend(b"BIN\0");
        glb.extend(blob);
        let path = std::env::temp_dir().join(format!("msh-{name}-{}.glb", std::process::id()));
        std::fs::write(&path, glb).unwrap();
        path
    }
    #[test]
    fn decodes_real_meshopt_streams_and_quantized_node_scale() {
        let path = compressed_fixture("quantized-codec", false, false);
        let data = crate::mesh::loader::load_mesh_with_colors(&path, None).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(data.face_indices, [[0, 1, 2]]);
        assert_eq!(data.positions, [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
    }
    #[test]
    fn malformed_and_oversized_compressed_data_fail_cleanly() {
        for (name, corrupt, huge) in [("bad-codec", true, false), ("huge-codec", false, true)] {
            let path = compressed_fixture(name, corrupt, huge);
            assert!(import(&path).is_err());
            std::fs::remove_file(path).unwrap();
        }
    }
}
