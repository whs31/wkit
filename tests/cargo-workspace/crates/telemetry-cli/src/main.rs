fn main() -> anyhow::Result<()> {
  let path = std::env::args()
    .nth(1)
    .ok_or_else(|| anyhow::anyhow!("usage: telemetry <file.csv>"))?;
  let text = std::fs::read_to_string(&path)?;
  let samples: Vec<_> = text.lines().filter_map(telemetry::Sample::parse).collect();
  match telemetry::max_altitude(&samples) {
    Some(max) => println!("{} samples, max altitude {max} m", samples.len()),
    None => println!("no samples"),
  }
  Ok(())
}
