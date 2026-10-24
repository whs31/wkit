fn main() {
    let frame: Vec<u8> = std::env::args()
        .skip(1)
        .filter_map(|b| u8::from_str_radix(&b, 16).ok())
        .collect();
    match packet_tools::decode(&frame) {
        Some(payload) => println!("{}", String::from_utf8_lossy(payload)),
        None => std::process::exit(1),
    }
}
