use clap::{Arg, ArgAction, Command};

pub fn subcommand() -> Command {
    Command::new("diff")
        .about("Compare two images for similarity")
        .arg(
            Arg::new("image1")
                .help("First image path")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("image2")
                .help("Second image path")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .short('t')
                .help("Similarity threshold percentage (0-100, default: 95)")
                .value_parser(clap::value_parser!(u8))
                .default_value("95"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output in JSON format")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("diff-output")
                .long("diff-output")
                .help("Save visual diff image highlighting differences"),
        )
}
