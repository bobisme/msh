use gltf::animation::{Interpolation, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../test-assets/RiggedSimple.glb".to_string());

    println!("Loading: {}\n", path);
    let (document, buffers, _images) = gltf::import(&path)?;

    // --- Overview ---
    let animations: Vec<_> = document.animations().collect();
    println!("=== Animation Overview ===");
    println!("Total animations: {}", animations.len());
    for anim in &animations {
        println!(
            "  [{}] name={:?}  channels={}  samplers={}",
            anim.index(),
            anim.name(),
            anim.channels().count(),
            anim.samplers().count(),
        );
    }
    println!();

    // --- Detailed per-animation dump ---
    for anim in &animations {
        println!(
            "=== Animation {} ({:?}) ===",
            anim.index(),
            anim.name().unwrap_or("<unnamed>")
        );

        for channel in anim.channels() {
            let target = channel.target();
            let node = target.node();
            let property = target.property();
            let sampler = channel.sampler();

            println!(
                "\n  Channel {}: node={} ({:?}) property={} interpolation={}",
                channel.index(),
                node.index(),
                node.name().unwrap_or("<unnamed>"),
                property_name(&property),
                interpolation_name(&sampler.interpolation()),
            );

            // Use the channel reader to get typed data
            let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

            // Read input timestamps
            let inputs: Vec<f32> = match reader.read_inputs() {
                Some(iter) => iter.collect(),
                None => {
                    println!("    (no input data)");
                    continue;
                }
            };

            println!(
                "    Keyframes: {}  time_range: [{:.4}, {:.4}]",
                inputs.len(),
                inputs.first().unwrap_or(&0.0),
                inputs.last().unwrap_or(&0.0),
            );

            // Print first few timestamps
            let show_count = inputs.len().min(8);
            print!("    Timestamps:");
            for t in &inputs[..show_count] {
                print!(" {:.4}", t);
            }
            if inputs.len() > show_count {
                print!(" ... ({} more)", inputs.len() - show_count);
            }
            println!();

            // Read output values
            match reader.read_outputs() {
                Some(outputs) => match outputs {
                    gltf::animation::util::ReadOutputs::Translations(iter) => {
                        let values: Vec<[f32; 3]> = iter.collect();
                        print_output_info("Translation", values.len(), &inputs, &sampler.interpolation());
                        // Print first few values
                        for (i, v) in values.iter().take(6).enumerate() {
                            println!("      [{}] ({:.4}, {:.4}, {:.4})", i, v[0], v[1], v[2]);
                        }
                        if values.len() > 6 {
                            println!("      ... ({} more)", values.len() - 6);
                        }
                    }
                    gltf::animation::util::ReadOutputs::Rotations(iter) => {
                        let values: Vec<[f32; 4]> = iter.into_f32().collect();
                        print_output_info("Rotation", values.len(), &inputs, &sampler.interpolation());
                        // Print first few values and check normalization
                        for (i, q) in values.iter().take(6).enumerate() {
                            let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                            println!(
                                "      [{}] ({:.4}, {:.4}, {:.4}, {:.4})  |q|={:.6}",
                                i, q[0], q[1], q[2], q[3], len
                            );
                        }
                        if values.len() > 6 {
                            println!("      ... ({} more)", values.len() - 6);
                        }
                        // Check if any quaternions need normalization
                        let max_deviation = values
                            .iter()
                            .map(|q| {
                                let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                                (len - 1.0).abs()
                            })
                            .fold(0.0f32, f32::max);
                        println!(
                            "    Max quaternion norm deviation from 1.0: {:.8}",
                            max_deviation
                        );
                        if max_deviation > 1e-5 {
                            println!("    WARNING: Quaternions need normalization!");
                        } else {
                            println!("    Quaternions are well-normalized.");
                        }
                    }
                    gltf::animation::util::ReadOutputs::Scales(iter) => {
                        let values: Vec<[f32; 3]> = iter.collect();
                        print_output_info("Scale", values.len(), &inputs, &sampler.interpolation());
                        for (i, v) in values.iter().take(6).enumerate() {
                            println!("      [{}] ({:.4}, {:.4}, {:.4})", i, v[0], v[1], v[2]);
                        }
                        if values.len() > 6 {
                            println!("      ... ({} more)", values.len() - 6);
                        }
                    }
                    gltf::animation::util::ReadOutputs::MorphTargetWeights(iter) => {
                        let values: Vec<f32> = iter.into_f32().collect();
                        print_output_info("MorphTargetWeights", values.len(), &inputs, &sampler.interpolation());
                        for (i, w) in values.iter().take(10).enumerate() {
                            println!("      [{}] {:.4}", i, w);
                        }
                        if values.len() > 10 {
                            println!("      ... ({} more)", values.len() - 10);
                        }
                    }
                },
                None => {
                    println!("    (no output data)");
                }
            }
        }
        println!();
    }

    // --- Sampler-level raw accessor info ---
    println!("=== Raw Accessor Details ===");
    for anim in &animations {
        for (si, sampler) in anim.samplers().enumerate() {
            let input_acc = sampler.input();
            let output_acc = sampler.output();
            println!(
                "Animation {} Sampler {}:",
                anim.index(),
                si
            );
            println!(
                "  Input accessor: index={} count={} type={:?} component_type={:?}",
                input_acc.index(),
                input_acc.count(),
                input_acc.data_type(),
                input_acc.dimensions(),
            );
            println!(
                "  Output accessor: index={} count={} type={:?} component_type={:?}",
                output_acc.index(),
                output_acc.count(),
                output_acc.data_type(),
                output_acc.dimensions(),
            );
            println!(
                "  Interpolation: {}",
                interpolation_name(&sampler.interpolation())
            );

            // For CubicSpline, output count should be 3x input count
            if matches!(sampler.interpolation(), Interpolation::CubicSpline) {
                let expected = input_acc.count() * 3;
                println!(
                    "  CubicSpline: input_count={} output_count={} expected_output={}  match={}",
                    input_acc.count(),
                    output_acc.count(),
                    expected,
                    output_acc.count() == expected,
                );
            }
            println!();
        }
    }

    // --- Skin/node info for context ---
    println!("=== Node Summary (for animation target context) ===");
    for node in document.nodes() {
        let (t, r, s) = node.transform().decomposed();
        println!(
            "  Node {} ({:?}): T=({:.2},{:.2},{:.2}) R=({:.2},{:.2},{:.2},{:.2}) S=({:.2},{:.2},{:.2})  children={}",
            node.index(),
            node.name().unwrap_or("<unnamed>"),
            t[0], t[1], t[2],
            r[0], r[1], r[2], r[3],
            s[0], s[1], s[2],
            node.children().count(),
        );
    }

    Ok(())
}

fn property_name(p: &Property) -> &'static str {
    match p {
        Property::Translation => "Translation",
        Property::Rotation => "Rotation",
        Property::Scale => "Scale",
        Property::MorphTargetWeights => "MorphTargetWeights",
    }
}

fn interpolation_name(i: &Interpolation) -> &'static str {
    match i {
        Interpolation::Linear => "Linear",
        Interpolation::Step => "Step",
        Interpolation::CubicSpline => "CubicSpline",
    }
}

fn print_output_info(
    kind: &str,
    output_count: usize,
    inputs: &[f32],
    interpolation: &Interpolation,
) {
    println!("    Output type: {}", kind);
    println!("    Output count: {}", output_count);

    if matches!(interpolation, Interpolation::CubicSpline) {
        // CubicSpline packs [in_tangent, value, out_tangent] per keyframe
        // so output_count = 3 * keyframe_count
        let expected = inputs.len() * 3;
        println!(
            "    CubicSpline tangent packing: {} values = {} keyframes x 3 (in_tangent, value, out_tangent).  Match: {}",
            output_count,
            inputs.len(),
            output_count == expected,
        );
        println!("    NOTE: gltf crate Reader already unpacks — values include tangents interleaved.");
    } else {
        let matches = output_count == inputs.len();
        println!(
            "    Output/input count match: {} (expected for {:?})",
            matches,
            interpolation,
        );
    }
}
