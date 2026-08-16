use std::{
    env,
    f64::consts::PI,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let file = File::create(out_dir.join("jeffreys_tables.rs")).expect("create Jeffreys tables");
    let mut out = BufWriter::new(file);

    write_probability_boundaries(&mut out);
    write_logit_boundaries(&mut out);
    write_representatives(&mut out);
}

fn write_probability_boundaries(out: &mut impl Write) {
    writeln!(out, "static PROBABILITY_BOUNDARIES_LOWER: [f32; 127] = [").unwrap();
    for k in 1..=127 {
        writeln!(out, "    {:e}f32,", probability_boundary(k) as f32).unwrap();
    }
    writeln!(out, "];\n").unwrap();
}

fn write_logit_boundaries(out: &mut impl Write) {
    writeln!(out, "static LOGIT_BOUNDARIES_LOWER: [f32; 127] = [").unwrap();
    for k in 1..=127 {
        let angle = PI * k as f64 / 512.0;
        let logit = 2.0 * angle.tan().ln();
        writeln!(out, "    {:e}f32,", logit as f32).unwrap();
    }
    writeln!(out, "];\n").unwrap();
}

fn write_representatives(out: &mut impl Write) {
    writeln!(
        out,
        "static REPRESENTATIVE_PROBABILITIES_LOWER: [f32; 128] = ["
    )
    .unwrap();

    for k in 0..128 {
        let lower = probability_boundary(k);
        let upper = probability_boundary(k + 1);
        let logit = (entropy(lower) - entropy(upper)) / (upper - lower);
        let representative = sigmoid(logit);
        writeln!(out, "    {:e}f32,", representative as f32).unwrap();
    }

    writeln!(out, "];\n").unwrap();
}

fn probability_boundary(k: usize) -> f64 {
    let angle = PI * k as f64 / 512.0;
    angle.sin().powi(2)
}

fn entropy(p: f64) -> f64 {
    match p {
        0.0 | 1.0 => 0.0,
        _ => -p * p.ln() - (1.0 - p) * (1.0 - p).ln(),
    }
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}
