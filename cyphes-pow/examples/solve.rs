//! Print every BeamHash III solution for an input and nonce.
//!
//!     cargo run --release -p cyphes-pow --features solver --example solve -- <input_hex> <nonce_hex>

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, nonce] = &args[..] else {
        eprintln!("usage: solve <input_hex> <nonce_hex>");
        std::process::exit(2);
    };
    let input = hex::decode(input).expect("input hex");
    let nonce: [u8; 8] = hex::decode(nonce)
        .expect("nonce hex")
        .try_into()
        .expect("8-byte nonce");

    let started = std::time::Instant::now();
    let solutions = cyphes_pow::solver::solve(&input, &nonce, [0; 4]);
    for s in &solutions {
        println!("{}", hex::encode(s));
    }
    eprintln!(
        "solutions: {} in {:.1?}",
        solutions.len(),
        started.elapsed()
    );
}
