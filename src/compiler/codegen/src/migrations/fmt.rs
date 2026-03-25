use sqlformat::{FormatOptions, QueryParams};

pub fn beautify(input: String) -> String {
    let opts = FormatOptions::<'_> {
        lines_between_queries: 2,
        ..FormatOptions::default()
    };

    sqlformat::format(&input, &QueryParams::None, &opts)
}
