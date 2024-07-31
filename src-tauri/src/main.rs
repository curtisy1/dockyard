// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::Command;

use rust_dock::{
    container::Container,
    version::Version,
    Docker,
};
use tauri::Manager;
use tokio::sync::mpsc;

mod docker_service;

struct AppState {
    containers: Vec<Container>,
}

impl AppState {
    fn default() -> Self {
        return AppState {
            containers: docker_service::get_containers(),
        };
    }
}

fn get_docker() -> Docker {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
        Ok(docker) => docker,
        Err(e) => {
            panic!("{}", e);
        }
    };

    return docker;
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn fetch_containers() -> Vec<Container> {
    return docker_service::get_containers();
}


#[tauri::command]
fn get_container(c_id: String) -> Container {
    let containers = docker_service::get_containers();

    return containers.iter().find(|c| c.Id == c_id).expect("Container not found").clone();
}

#[tauri::command]
fn fetch_container_info(state: tauri::State<AppState>, c_id: String) -> serde_json::Value {
    let container = state
        .containers
        .iter()
        .find(|c| c.Id == c_id)
        .expect("Can't find container withd Id {c_id}");

    return docker_service::get_container_info(container);
}

#[tauri::command]
fn fetch_version() -> Version {
    return docker_service::get_version();
}

#[tauri::command]
async fn stream_docker_logs(
    app_handle: tauri::AppHandle,
    container_id: String,
) -> Result<(), String> {
    let docker = Docker::connect("unix:///var/run/docker.sock").map_err(|e| e.to_string())?;

    let (sender, mut receiver) = mpsc::channel(100);

    tokio::spawn(async move {
        if let Err(err) = docker.stream_container_logs(&container_id, sender).await {
            eprintln!("Error streaming logs: {}", err);
        }
    });

    tokio::spawn(async move {
        while let Some(log_chunk) = receiver.recv().await {
            app_handle
                .emit_all("log_chunk", log_chunk)
                .expect("Failed to emit log chunk");
        }
    });

    Ok(())
}

#[tauri::command]
fn container_operation(state: tauri::State<AppState>, c_id: String, op_type: String) -> String {
    let mut d = get_docker();

    let container = state
        .containers
        .iter()
        .find(|c| c.Id == c_id)
        .expect("Can't find container");


    // TODO: Improve error handling
    let res = match op_type.as_str() {
        "delete" => match d.delete_container(&c_id) {
            Ok(_) => &format!("Deleted container"),
            Err(e) => &format!("Failed to delete container: {}", e.to_string()),
        },
        "start" => match d.start_container(&c_id) {
            Ok(_) => &format!("Container started"),
            Err(e) => &format!("Failed to delete container: {}", e.to_string()),
        },
        "stop" => match d.stop_container(&c_id) {
            Ok(_) => &format!("Container stopped"),
            Err(e) => &format!("Failed to delete container: {}", e.to_string()),
        },
        "restart" => {
            let _ = match d.stop_container(&c_id) {
                Ok(_) => &format!("Container restarted"),
                Err(e) => &format!("Failed to delete container: {}", e.to_string()),
            };

            let res = match d.start_container(&c_id) {
                Ok(_) => &format!("Container restarted"),
                Err(e) => &format!("Failed to delete container: {}", e.to_string()),
            };

            return res.to_string();
        }
        "web" => {
            let path = format!(
                "http://0.0.0.0:{}",
                container.Ports[0].PublicPort.expect("port not available")
            );
            match open::that(path.clone()) {
                Ok(()) => &format!("Opening '{}'.", path),
                Err(err) => &format!("An error occurred when opening '{}': {}", path, err),
            }
        }

        "exec" => {
            // TODO: Make it platform/os agnostic
            let container_name = container.Names[0].replace("/", ""); // Replace with your container name
            let docker_command = format!("docker exec -it {} sh", container_name);

            // Using gnome-terminal to run the docker command
            let mut command = Command::new("gnome-terminal");

            // -e flag is used to execute the command in gnome-terminal
            let args = ["--", "bash", "-c", &docker_command];

            command.args(&args);
            match command.spawn() {
                Ok(_) => "",
                Err(err) => &format!("Cannot run exec command: {}", err.to_string()),
            }
        }
        _ => "Invalid operation type",
    };

    return res.to_string();
}

fn main() {
    let state = AppState::default();

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            fetch_containers,
            fetch_version,
            fetch_container_info,
            stream_docker_logs,
            container_operation,
            get_container
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
