use anyhow::{Context, Result, anyhow};
use dotenvy::dotenv;
use log::{error, info, warn};
use reqwest::StatusCode;
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tokio::fs as tokio_fs;
use tokio::io::AsyncReadExt;
use walkdir::WalkDir;
use env_logger::Env;

#[derive(Debug, Clone)]
struct Config {
    api_key: Option<String>,
    api_address: String,
    api_refresh_address: String,
    dropbox_path: Option<String>,
    app_key: String,
    app_secret: String,
    refresh_token: String,
    dropbox_dir: String,
    uploaded_files_log: PathBuf,
    uploaded_directory: PathBuf,
    current_directory: PathBuf,
    file_extensions: Vec<String>,
    recurse: bool,
    skip_dirs: HashSet<String>,
    short_token_file: PathBuf,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv().ok();
        let get = |k: &str| env::var(k).with_context(|| format!("Missing env var `{}`", k));

        let api_key = env::var("API_KEY").ok();
        let dropbox_path = env::var("DROPBOX_PATH").ok();
        let api_address = get("API_ADDRESS")?;
        let api_refresh_address = get("API_REFRESH_ADDRESS")?;
        let app_key = get("APP_KEY")?;
        let app_secret = get("APP_SECRET")?;
        let refresh_token = get("REFRESH_TOKEN")?;
        let dropbox_dir = get("DROPBOX_DIR")?;
        let uploaded_files_log = PathBuf::from(get("UPLOADED_FILES_LOG")?);
        let uploaded_directory = PathBuf::from(get("UPLOADED_DIRECTORY")?);
        let current_directory = PathBuf::from(get("CURRENT_DIRECTORY")?);
        let file_extensions = env::var("FILE_EXTENSIONS")?
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let recurse = env::var("RECURSE")
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "t"))
            .unwrap_or(false);
        let skip_dirs = env::var("SKIP_DIRS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect::<HashSet<_>>();
        let short_token_file = PathBuf::from(get("SHORT_TOKEN_FILE")?);

        Ok(Self {
            api_key,
            api_address,
            api_refresh_address,
            dropbox_path,
            app_key,
            app_secret,
            refresh_token,
            dropbox_dir,
            uploaded_files_log,
            uploaded_directory,
            current_directory,
            file_extensions,
            recurse,
            skip_dirs,
            short_token_file,
        })
    }
}

fn ensure_log_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(path)?;
    }
    Ok(())
}

fn check_uploaded_log(log_path: &Path, file_path: &Path) -> Result<bool> {
    ensure_log_exists(log_path)?;
    let f = File::open(log_path)?;
    let reader = BufReader::new(f);
    for line in reader.lines() {
        if line? == file_path.to_string_lossy() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn log_uploaded_file(log_path: &Path, file_path: &Path) -> Result<()> {
    ensure_log_exists(log_path)?;
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)?;
    writeln!(f, "{}", file_path.to_string_lossy())?;
    Ok(())
}

fn extract_filename(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .ok_or_else(|| anyhow!("No filename in path"))?
        .to_string_lossy()
        .to_string())
}

fn move_file(source: &Path, destination_dir: &Path) -> Result<()> {
    fs::create_dir_all(destination_dir)?;
    let dest = destination_dir.join(source.file_name().ok_or_else(|| anyhow!("No filename"))?);
    fs::rename(source, &dest)
        .with_context(|| format!("Failed to move {:?} to {:?}", source, dest))?;
    Ok(())
}

fn sanitize_filename_spaces(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("No file name"))?
        .to_string_lossy();
    if !file_name.contains(' ') {
        return Ok(path.to_path_buf());
    }
    let new_name = file_name.replace(' ', "_");
    let new_path = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(new_name);
    fs::rename(path, &new_path)?;
    info!("Renamed file: {:?} -> {:?}", path, new_path);
    Ok(new_path)
}

fn collect_files(config: &Config) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let exts: HashSet<String> = config
        .file_extensions
        .iter()
        .map(|e| e.to_lowercase())
        .collect();

    let walker = if config.recurse {
        WalkDir::new(&config.current_directory)
            .into_iter()
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>()
    } else {
        fs::read_dir(&config.current_directory)?
            .filter_map(|e| e.ok())
            .map(|e| WalkDir::new(e.path()).into_iter().next().unwrap().unwrap())
            .collect()
    };

    for entry in walker {
        let path = entry.path();
        if entry.file_type().is_dir() {
            if config
                .skip_dirs
                .contains(&entry.file_name().to_string_lossy().to_string())
            {
                continue;
            }
            continue;
        }
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if exts.contains(&format!(".{}", ext.to_lowercase()))
                || exts.contains(&ext.to_lowercase())
            {
                let sanitized = sanitize_filename_spaces(path)?;
                files.push(sanitized);
            }
        }
    }
    Ok(files)
}

async fn read_short_token_or_create(config: &Config) -> Result<String> {
    if config.short_token_file.exists() {
        let mut f = tokio_fs::File::open(&config.short_token_file).await?;
        let mut buf = String::new();
        f.read_to_string(&mut buf).await?;
        return Ok(buf.trim().to_string());
    }

    warn!("short_token.txt not found, requesting new token...");
    let token = get_new_short_token(config).await?;
    write_short_token(&config.short_token_file, &token).await?;
    Ok(token)
}

async fn write_short_token(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio_fs::create_dir_all(parent).await.ok();
    }
    tokio_fs::write(path, token)
        .await
        .with_context(|| format!("Write short token file: {:?}", path))
}

async fn get_new_short_token(config: &Config) -> Result<String> {
    #[derive(Deserialize)]
    struct Resp {
        access_token: String,
    }

    info!("Requesting new short-lived access token...");
    let client = reqwest::Client::new();
    let resp = client
        .post(&config.api_refresh_address)
        .form(&[
            ("refresh_token", config.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
            ("client_id", config.app_key.as_str()),
            ("client_secret", config.app_secret.as_str()),
        ])
        .send()
        .await
        .context("Token refresh request failed")?;

    if !resp.status().is_success() {
        return Err(anyhow!("Token refresh HTTP {}", resp.status()));
    }

    let body: Resp = resp.json().await.context("Parsing token refresh JSON")?;
    Ok(body.access_token)
}

async fn upload_file_once(
    client: &reqwest::Client,
    config: &Config,
    local_file: &Path,
    short_token: &str,
) -> Result<()> {
    let path_arg = format!("{}/{}", config.dropbox_dir, extract_filename(local_file)?);
    let dropbox_arg = serde_json::json!({
        "autorename": false,
        "mode": "add",
        "mute": false,
        "path": path_arg,
        "strict_conflict": false,
    });

    let mut file = tokio_fs::File::open(local_file).await?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await?;

    let req = client
        .post(&config.api_address)
        .header("Authorization", format!("Bearer {}", short_token))
        .header("Content-Type", "application/octet-stream")
        .header("Dropbox-API-Arg", dropbox_arg.to_string())
        .body(buf);

    let resp = req.send().await?;
    match resp.status() {
        s if s.is_success() => {
            info!("Uploaded {:?} successfully (HTTP {})", local_file, s);
            Ok(())
        }
        StatusCode::UNAUTHORIZED => Err(anyhow!("unauthorized")),
        s => {
            let text = resp.text().await.unwrap_or_default();
            Err(anyhow!("Upload failed: HTTP {} - {}", s, text))
        }
    }
}

async fn send_file(config: &Config, local_file: &Path) -> Result<()> {
    if check_uploaded_log(&config.uploaded_files_log, local_file)? {
        info!("Already uploaded, skipping: {:?}", local_file);
        return Ok(());
    }

    let client = reqwest::Client::new();

    let mut token = read_short_token_or_create(config).await?;
    match upload_file_once(&client, config, local_file, &token).await {
        Ok(()) => {
            log_uploaded_file(&config.uploaded_files_log, local_file)?;
            move_file(local_file, &config.uploaded_directory)?;
            Ok(())
        }
        Err(e) if e.to_string().contains("unauthorized") => {
            warn!("Token expired/unauthorized. Refreshing...");
            token = get_new_short_token(config).await?;
            write_short_token(&config.short_token_file, &token).await?;
            upload_file_once(&client, config, local_file, &token).await?;
            log_uploaded_file(&config.uploaded_files_log, local_file)?;
            move_file(local_file, &config.uploaded_directory)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("trace")).init();
    let config = Config::from_env()?;

    info!("Starting Dropbox backup service");

    fs::create_dir_all(&config.uploaded_directory).ok();
    ensure_log_exists(&config.uploaded_files_log).ok();

    let files = collect_files(&config)?;

    if files.is_empty() {
        info!("No files matched the provided extensions.");
        return Ok(());
    }

    for file in files {
        if let Err(e) = send_file(&config, &file).await {
            error!("Failed to process {:?}: {}", file, e);
        }
    }

    info!("Done.");
    Ok(())
}
