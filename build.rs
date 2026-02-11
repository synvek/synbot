use std::process::Command;
use std::env;
use std::path::Path;

fn npm_command() -> Command {
    // On Windows, npm is a .cmd file
    if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(&["/C", "npm"]);
        cmd
    } else {
        Command::new("npm")
    }
}

fn main() {
    // Only build frontend in release mode
    let profile = env::var("PROFILE").unwrap_or_default();
    
    if profile == "release" {
        println!("cargo:warning=Building frontend for release...");
        
        let frontend_dir = Path::new("web");
        
        // Check if frontend directory exists
        if !frontend_dir.exists() {
            println!("cargo:warning=Frontend directory not found, skipping frontend build");
            return;
        }
        
        // Check if node_modules exists, if not run npm install
        let node_modules = frontend_dir.join("node_modules");
        if !node_modules.exists() {
            println!("cargo:warning=Installing frontend dependencies...");
            let mut install_cmd = npm_command();
            let install_status = install_cmd
                .args(&["install"])
                .current_dir(frontend_dir)
                .status();
            
            match install_status {
                Ok(status) if status.success() => {
                    println!("cargo:warning=Frontend dependencies installed successfully");
                }
                Ok(status) => {
                    panic!("npm install failed with status: {}", status);
                }
                Err(e) => {
                    panic!("Failed to run npm install: {}. Make sure Node.js and npm are installed.", e);
                }
            }
        }
        
        // Build the frontend
        println!("cargo:warning=Building frontend with npm run build...");
        let mut build_cmd = npm_command();
        let build_output = build_cmd
            .args(&["run", "build"])
            .current_dir(frontend_dir)
            .output();
        
        match build_output {
            Ok(output) if output.status.success() => {
                println!("cargo:warning=Frontend built successfully");
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("npm run build stdout:\n{}", stdout);
                eprintln!("npm run build stderr:\n{}", stderr);
                panic!("npm run build failed with status: {}\nstdout: {}\nstderr: {}", 
                       output.status, stdout, stderr);
            }
            Err(e) => {
                panic!("Failed to run npm run build: {}. Make sure Node.js and npm are installed.", e);
            }
        }
        
        // Verify dist directory was created
        let dist_dir = frontend_dir.join("dist");
        if !dist_dir.exists() {
            panic!("Frontend build completed but dist directory not found");
        }
        
        println!("cargo:warning=Frontend build complete, assets will be embedded");
    } else {
        println!("cargo:warning=Skipping frontend build in {} mode", profile);
    }
    
    // Tell cargo to rerun this script if frontend files change
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/package.json");
    println!("cargo:rerun-if-changed=web/vite.config.ts");
}
