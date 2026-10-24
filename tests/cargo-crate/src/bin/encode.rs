fn main() {
    let payload = std::env::args().nth(1).unwrap_or_default();
    println!("{:02x?}", packet_tools::encode(payload.as_bytes()));
}
