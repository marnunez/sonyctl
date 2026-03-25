/// Auto-detect a connected Sony headphone via bluetoothctl.
use std::process::Command;

pub fn find_sony_mac() -> Option<String> {
    let output = Command::new("bluetoothctl")
        .args(["devices", "Connected"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let upper = line.to_uppercase();
        if upper.contains("WH-1000XM") || upper.contains("WF-1000XM") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }
    }
    None
}
