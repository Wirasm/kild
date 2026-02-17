use clap::ArgMatches;
use kild_peek_core::diff::{DiffRequest, compare_images};
use kild_peek_core::events;
use tracing::{error, info};

pub fn handle_diff_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let image1 = matches.get_one::<String>("image1").unwrap();
    let image2 = matches.get_one::<String>("image2").unwrap();
    let threshold_percent = *matches.get_one::<u8>("threshold").unwrap_or(&95);
    let json_output = matches.get_flag("json");
    let diff_output = matches.get_one::<String>("diff-output");

    let threshold = (threshold_percent as f64) / 100.0;

    info!(
        event = "peek.cli.diff_started",
        image1 = image1,
        image2 = image2,
        threshold = threshold,
        diff_output = ?diff_output
    );

    let mut request = DiffRequest::new(image1, image2).with_threshold(threshold);
    if let Some(path) = diff_output {
        request = request.with_diff_output(path);
    }

    match compare_images(&request) {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = match result.is_similar() {
                    true => "SIMILAR",
                    false => "DIFFERENT",
                };
                println!("Image comparison: {}", status);
                println!("  Similarity: {}", result.similarity_percent());
                println!("  Threshold: {}%", threshold_percent);
                println!("  Image 1: {}x{}", result.width1(), result.height1());
                println!("  Image 2: {}x{}", result.width2(), result.height2());
                if let Some(path) = result.diff_output_path() {
                    println!("  Diff saved: {}", path);
                }
            }

            info!(
                event = "peek.cli.diff_completed",
                similarity = result.similarity(),
                is_similar = result.is_similar()
            );

            // Exit with code 1 if images are different (for CI/scripting)
            if !result.is_similar() {
                std::process::exit(1);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to compare images: {}", e);
            error!(event = "peek.cli.diff_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
