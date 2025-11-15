// build.rs - Set build-time environment variables

use std::env;

fn main() {
    // Get BUILD_TIMESTAMP from environment or generate current time
    let timestamp = env::var("BUILD_TIMESTAMP")
        .unwrap_or_else(|_| {
            // Generate timestamp in readable format
            let output = std::process::Command::new("date")
                .arg("+%Y-%m-%d %H:%M:%S")
                .output();
            
            match output {
                Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
                Err(_) => "build-time-unknown".to_string(),
            }
        });
    
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);
    println!("cargo:rerun-if-env-changed=BUILD_TIMESTAMP");
}
