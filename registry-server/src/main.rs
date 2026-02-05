use axum::{
    extract::{Path, State, Multipart, Query, DefaultBodyLimit},
    http::{StatusCode, header, HeaderMap},
    routing::{get, post},
    Json, Router, response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use sqlx::{SqlitePool, Row};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct RegistryState {
    db: SqlitePool,
    data_dir: PathBuf,
    base_url: String,
}

#[derive(Serialize)]
struct PackageList {
    packages: Vec<String>,
}

#[derive(Serialize)]
struct VersionList {
    name: String,
    versions: Vec<String>,
}

#[derive(Serialize)]
struct PackageMeta {
    name: String,
    version: String,
    description: String,
    license: Option<String>,
    size: u64,
    sha256: String,
    download_url: String,
    wit_url: Option<String>,
    #[serde(default)]
    dependencies: Vec<DependencyResponse>,
    #[serde(default)]
    targets: Vec<String>,
    repository: Option<String>,
    published_at: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct DependencyResponse {
    name: String,
    version: String,
    #[serde(default)]
    optional: bool,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
struct SearchResponse {
    packages: Vec<PackageMeta>,
    total: usize,
}


#[tokio::main]
async fn main() {
    let base_url = std::env::var("REGISTRY_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let data_dir = std::env::var("REGISTRY_DATA_DIR")
        .unwrap_or_else(|_| "./registry-data".to_string());
    let data_dir = PathBuf::from(data_dir);

    std::fs::create_dir_all(data_dir.join("artifacts")).expect("Failed to create data dir");

    let db_path = data_dir.join("registry.sqlite3");
    let db = init_db(&db_path).await.expect("Failed to initialize database");
    seed_tokens_from_env(&db).await.expect("Failed to seed tokens");

    let max_upload_mb = std::env::var("REGISTRY_MAX_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50);
    let max_upload_bytes = max_upload_mb * 1024 * 1024;

    let state = RegistryState {
        db,
        data_dir,
        base_url,
    };

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/v1/packages", post(publish_package))
        .route("/api/v1/packages/:name/versions", get(list_versions))
        .route("/api/v1/packages/:name/:version", get(get_meta))
        .route("/api/v1/stats", get(get_stats))
        .route("/api/v1/search", get(search_packages))
        .route("/packages", get(list_packages))
        .route("/packages/:name/:version/artifact.wasm", get(get_artifact))
        .route("/health", get(health_check))
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);
    println!("Run Registry v0");
    println!("Listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "ok"
}

async fn root_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html")],
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Run Registry</title>
    <style>
        body { font-family: system-ui, sans-serif; max-width: 800px; margin: 50px auto; padding: 20px; }
        h1 { color: #333; }
        code { background: #f4f4f4; padding: 2px 6px; border-radius: 3px; }
        pre { background: #f4f4f4; padding: 15px; border-radius: 5px; overflow-x: auto; }
        a { color: #0066cc; }
    </style>
</head>
<body>
    <h1>Run Registry</h1>
    <p>WASI component registry for <a href="https://github.com/Esubaalew/run">Run 2.0</a>.</p>
    
    <h2>API Endpoints</h2>
    <ul>
        <li><code>GET /health</code> - Health check</li>
        <li><code>GET /packages</code> - List all packages</li>
        <li><code>GET /api/v1/packages/:name/versions</code> - List versions</li>
        <li><code>GET /api/v1/packages/:name/:version</code> - Get package metadata</li>
        <li><code>GET /api/v1/search?q=query</code> - Search packages</li>
        <li><code>GET /api/v1/stats</code> - Registry statistics</li>
        <li><code>POST /api/v1/packages</code> - Publish (requires auth)</li>
    </ul>

    <h2>Usage</h2>
    <pre>run v2 install namespace:package@1.0.0</pre>
    
    <p><a href="https://github.com/Esubaalew/run">Documentation</a></p>
</body>
</html>"#
    )
}

async fn get_stats(
    State(state): State<RegistryState>,
) -> Json<serde_json::Value> {
    let (package_count, version_count) = match stats_counts(&state.db).await {
        Ok(counts) => counts,
        Err(err) => {
            eprintln!("Failed to fetch stats: {}", err);
            (0, 0)
        }
    };

    let downloads = total_downloads(&state.db).await.unwrap_or(0);

    Json(serde_json::json!({
        "package_count": package_count,
        "version_count": version_count,
        "download_count": downloads,
        "uptime_percent": 100.0,
    }))
}

async fn list_packages(
    State(state): State<RegistryState>,
) -> Json<PackageList> {
    let names = fetch_package_names(&state.db).await.unwrap_or_default();
    Json(PackageList { packages: names })
}

async fn list_versions(
    Path(name): Path<String>,
    State(state): State<RegistryState>,
) -> Result<Json<VersionList>, StatusCode> {
    let versions = fetch_versions(&state.db, &name).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if versions.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(VersionList {
        name: name.clone(),
        versions,
    }))
}

async fn get_meta(
    Path((name, version)): Path<(String, String)>,
    State(state): State<RegistryState>,
) -> Result<Json<PackageMeta>, StatusCode> {
    let meta = fetch_package_meta(&state, &name, &version).await
        .map_err(|err| {
            eprintln!("get_meta failed: {}", err);
            StatusCode::NOT_FOUND
        })?;
    Ok(Json(meta))
}

async fn get_artifact(
    Path((name, version)): Path<(String, String)>,
    State(state): State<RegistryState>,
) -> Result<impl IntoResponse, StatusCode> {
    let sha256 = fetch_sha256(&state.db, &name, &version).await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let artifact_path = state.data_dir.join("artifacts").join(format!("{}.wasm", sha256));
    let data = tokio::fs::read(&artifact_path).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let _ = increment_download(&state.db, &name, &version).await;

    Ok((
        [(header::CONTENT_TYPE, "application/wasm")],
        data
    ))
}

async fn publish_package(
    State(state): State<RegistryState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut description = String::new();
    let mut license: Option<String> = None;
    let mut artifact_data: Option<Vec<u8>> = None;
    let mut repository: Option<String> = None;
    let mut wit_url: Option<String> = None;
    let mut dependencies: Option<Vec<DependencyResponse>> = None;
    let mut targets: Option<Vec<String>> = None;
    let mut expected_sha256: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let field_name = field.name().unwrap_or("").to_string();
        
        match field_name.as_str() {
            "name" => {
                name = Some(field.text().await.unwrap());
            }
            "version" => {
                version = Some(field.text().await.unwrap());
            }
            "description" => {
                description = field.text().await.unwrap();
            }
            "license" => {
                license = Some(field.text().await.unwrap());
            }
            "repository" => {
                repository = Some(field.text().await.unwrap());
            }
            "wit_url" => {
                wit_url = Some(field.text().await.unwrap());
            }
            "dependencies" => {
                let raw = field.text().await.unwrap();
                dependencies = serde_json::from_str(&raw).ok();
            }
            "targets" => {
                let raw = field.text().await.unwrap();
                targets = serde_json::from_str(&raw).ok();
            }
            "sha256" => {
                expected_sha256 = Some(field.text().await.unwrap());
            }
            "component" | "artifact" => {
                artifact_data = Some(field.bytes().await.unwrap().to_vec());
            }
            _ => {}
        }
    }

    let name = name.ok_or(StatusCode::BAD_REQUEST)?;
    let version = version.ok_or(StatusCode::BAD_REQUEST)?;
    let data = artifact_data.ok_or(StatusCode::BAD_REQUEST)?;
    let namespace = parse_namespace(&name).ok_or(StatusCode::BAD_REQUEST)?;

    let token = bearer_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let allowed_namespace = authorize_publish(&state.db, &token).await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if allowed_namespace != "*" && allowed_namespace != namespace {
        return Err(StatusCode::FORBIDDEN);
    }

    // IMMUTABILITY: Ban "latest" and other mutable tags
    if version == "latest" || version == "dev" || version == "stable" {
        eprintln!("REJECTED: mutable tag '{}' is banned", version);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate semver format
    if semver::Version::parse(&version).is_err() {
        eprintln!("REJECTED: invalid semver '{}'", version);
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let sha256 = hex::encode(hasher.finalize());

    if let Some(expected) = expected_sha256 {
        if expected != sha256 {
            eprintln!("REJECTED: sha256 mismatch expected={} actual={}", expected, sha256);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let size = data.len() as u64;
    let published_at = chrono::Utc::now().timestamp() as u64;

    let targets = targets.unwrap_or_else(|| vec!["wasm32-wasip2".to_string()]);
    let deps = dependencies.unwrap_or_default();
    let deps_json = serde_json::to_string(&deps).unwrap_or_else(|_| "[]".to_string());
    let targets_json = serde_json::to_string(&targets).unwrap_or_else(|_| "[]".to_string());

    let artifact_path = state.data_dir.join("artifacts").join(format!("{}.wasm", sha256));
    if !artifact_path.exists() {
        tokio::fs::write(&artifact_path, &data).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let package_id = upsert_package(&mut tx, &name, &namespace, &description, &license, &repository)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM versions WHERE package_id = ? AND version = ?"
    )
    .bind(package_id)
    .bind(&version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if exists.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    sqlx::query(
        "INSERT INTO versions (package_id, version, description, license, repository, size, sha256, published_at, dependencies, targets, wit_url) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(package_id)
    .bind(&version)
    .bind(&description)
    .bind(&license)
    .bind(&repository)
    .bind(size as i64)
    .bind(&sha256)
    .bind(published_at as i64)
    .bind(deps_json)
    .bind(targets_json)
    .bind(&wit_url)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    println!("Published {}@{}", name, version);

    Ok(StatusCode::CREATED)
}

async fn search_packages(
    Query(query): Query<SearchQuery>,
    State(state): State<RegistryState>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT p.name, p.description, p.license, p.repository, v.version, v.sha256, v.size, v.published_at, v.dependencies, v.targets, v.wit_url \
         FROM packages p \
         JOIN versions v ON v.package_id = p.id \
         WHERE p.name LIKE ? \
         ORDER BY p.name"
    )
    .bind(format!("%{}%", query.q))
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut grouped: HashMap<String, Vec<PackageMeta>> = HashMap::new();
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        let version: String = row.try_get("version").unwrap_or_default();
        let description: String = row.try_get("description").unwrap_or_default();
        let license: Option<String> = row.try_get("license").ok();
        let repository: Option<String> = row.try_get("repository").ok();
        let sha256: String = row.try_get("sha256").unwrap_or_default();
        let size: i64 = row.try_get("size").unwrap_or(0);
        let published_at: Option<i64> = row.try_get("published_at").ok();
        let deps_json: Option<String> = row.try_get("dependencies").ok();
        let targets_json: Option<String> = row.try_get("targets").ok();
        let wit_url: Option<String> = row.try_get("wit_url").ok();

        let dependencies = deps_json
            .and_then(|d| serde_json::from_str::<Vec<DependencyResponse>>(&d).ok())
            .unwrap_or_default();
        let targets = targets_json
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_else(|| vec!["wasm32-wasip2".to_string()]);

        let download_url = build_download_url(&state.base_url, &name, &version);

        let meta = PackageMeta {
            name: name.clone(),
            version: version.clone(),
            description,
            license,
            size: size as u64,
            sha256,
            download_url,
            wit_url,
            dependencies,
            targets,
            repository,
            published_at: published_at.map(|v| v as u64),
        };
        grouped.entry(name).or_default().push(meta);
    }

    let mut results = Vec::new();
    for (_name, mut versions) in grouped {
        versions.sort_by(|a, b| semver::Version::parse(&a.version).ok().cmp(&semver::Version::parse(&b.version).ok()));
        if let Some(latest) = versions.pop() {
            results.push(latest);
        }
    }

    let total = results.len();
    let slice = results.into_iter().skip(offset).take(limit).collect();

    Ok(Json(SearchResponse { packages: slice, total }))
}

async fn init_db(db_path: &FsPath) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);

    let db = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS packages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            namespace TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            license TEXT,
            repository TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(&db).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            package_id INTEGER NOT NULL,
            version TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            license TEXT,
            repository TEXT,
            size INTEGER NOT NULL,
            sha256 TEXT NOT NULL,
            published_at INTEGER NOT NULL,
            dependencies TEXT,
            targets TEXT,
            wit_url TEXT,
            UNIQUE(package_id, version)
        )"
    ).execute(&db).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS api_keys (
            token TEXT PRIMARY KEY,
            namespace TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            revoked INTEGER NOT NULL DEFAULT 0
        )"
    ).execute(&db).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS downloads (
            package_id INTEGER NOT NULL,
            version TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            UNIQUE(package_id, version)
        )"
    ).execute(&db).await?;

    Ok(db)
}

async fn seed_tokens_from_env(db: &SqlitePool) -> Result<(), sqlx::Error> {
    if let Ok(admin_token) = std::env::var("REGISTRY_ADMIN_TOKEN") {
        insert_token_if_missing(db, &admin_token, "*").await?;
    }

    if let Ok(tokens) = std::env::var("REGISTRY_TOKENS") {
        for entry in tokens.split(',') {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.splitn(2, ':');
            let namespace = parts.next().unwrap_or("").trim();
            let token = parts.next().unwrap_or("").trim();
            if !namespace.is_empty() && !token.is_empty() {
                insert_token_if_missing(db, token, namespace).await?;
            }
        }
    }
    Ok(())
}

async fn insert_token_if_missing(db: &SqlitePool, token: &str, namespace: &str) -> Result<(), sqlx::Error> {
    let existing = sqlx::query_scalar::<_, String>("SELECT token FROM api_keys WHERE token = ?")
        .bind(token)
        .fetch_optional(db)
        .await?;
    if existing.is_none() {
        sqlx::query("INSERT INTO api_keys (token, namespace, created_at, revoked) VALUES (?, ?, ?, 0)")
            .bind(token)
            .bind(namespace)
            .bind(chrono::Utc::now().timestamp() as i64)
            .execute(db)
            .await?;
    }
    Ok(())
}

fn parse_namespace(name: &str) -> Option<String> {
    let mut parts = name.split(':');
    let namespace = parts.next()?;
    let remainder = parts.next()?;
    if namespace.is_empty() || remainder.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(namespace.to_string())
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

async fn authorize_publish(db: &SqlitePool, token: &str) -> Result<String, sqlx::Error> {
    let row = sqlx::query("SELECT namespace, revoked FROM api_keys WHERE token = ?")
        .bind(token)
        .fetch_optional(db)
        .await?;
    if let Some(row) = row {
        let revoked: i64 = row.try_get("revoked").unwrap_or(0);
        if revoked != 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        let namespace: String = row.try_get("namespace").unwrap_or_default();
        return Ok(namespace);
    }
    Err(sqlx::Error::RowNotFound)
}

async fn upsert_package(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    name: &str,
    namespace: &str,
    description: &str,
    license: &Option<String>,
    repository: &Option<String>,
) -> Result<i64, sqlx::Error> {
    let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM packages WHERE name = ?")
        .bind(name)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    sqlx::query(
        "INSERT INTO packages (name, namespace, description, license, repository, created_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(name)
    .bind(namespace)
    .bind(description)
    .bind(license)
    .bind(repository)
    .bind(chrono::Utc::now().timestamp() as i64)
    .execute(&mut **tx)
    .await?;

    let id = sqlx::query_scalar::<_, i64>("SELECT id FROM packages WHERE name = ?")
        .bind(name)
        .fetch_one(&mut **tx)
        .await?;
    Ok(id)
}

async fn fetch_package_names(db: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query("SELECT name FROM packages ORDER BY name").fetch_all(db).await?;
    Ok(rows.into_iter().filter_map(|r| r.try_get("name").ok()).collect())
}

async fn fetch_versions(db: &SqlitePool, name: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT v.version FROM versions v JOIN packages p ON v.package_id = p.id WHERE p.name = ? ORDER BY v.version"
    )
    .bind(name)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(|r| r.try_get("version").ok()).collect())
}

async fn fetch_sha256(db: &SqlitePool, name: &str, version: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT v.sha256 FROM versions v JOIN packages p ON v.package_id = p.id WHERE p.name = ? AND v.version = ?"
    )
    .bind(name)
    .bind(version)
    .fetch_one(db)
    .await
}

async fn fetch_package_meta(state: &RegistryState, name: &str, version: &str) -> Result<PackageMeta, sqlx::Error> {
    let row = sqlx::query(
        "SELECT p.name, v.version, v.description, v.license, v.repository, v.size, v.sha256, v.published_at, v.dependencies, v.targets, v.wit_url \
         FROM packages p JOIN versions v ON v.package_id = p.id \
         WHERE p.name = ? AND v.version = ?"
    )
    .bind(name)
    .bind(version)
    .fetch_one(&state.db)
    .await?;

    let name: String = row.try_get("name")?;
    let version: String = row.try_get("version")?;
    let description: String = row.try_get("description")?;
    let license: Option<String> = row.try_get("license").ok();
    let repository: Option<String> = row.try_get("repository").ok();
    let size: i64 = row.try_get("size")?;
    let sha256: String = row.try_get("sha256")?;
    let published_at: i64 = row.try_get("published_at")?;
    let deps_json: Option<String> = row.try_get("dependencies").ok();
    let targets_json: Option<String> = row.try_get("targets").ok();
    let wit_url: Option<String> = row.try_get("wit_url").ok();

    let dependencies = deps_json
        .and_then(|d| serde_json::from_str::<Vec<DependencyResponse>>(&d).ok())
        .unwrap_or_default();
    let targets = targets_json
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_else(|| vec!["wasm32-wasip2".to_string()]);

    let download_url = build_download_url(&state.base_url, &name, &version);

    Ok(PackageMeta {
        name,
        version,
        description,
        license,
        size: size as u64,
        sha256,
        download_url,
        wit_url,
        dependencies,
        targets,
        repository,
        published_at: Some(published_at as u64),
    })
}

fn build_download_url(base_url: &str, name: &str, version: &str) -> String {
    let encoded_name = urlencoding::encode(name);
    format!("{}/packages/{}/{}/artifact.wasm", base_url, encoded_name, version)
}

async fn increment_download(db: &SqlitePool, name: &str, version: &str) -> Result<(), sqlx::Error> {
    let package_id = sqlx::query_scalar::<_, i64>("SELECT id FROM packages WHERE name = ?")
        .bind(name)
        .fetch_one(db)
        .await?;

    sqlx::query("INSERT OR IGNORE INTO downloads (package_id, version, count) VALUES (?, ?, 0)")
        .bind(package_id)
        .bind(version)
        .execute(db)
        .await?;

    sqlx::query("UPDATE downloads SET count = count + 1 WHERE package_id = ? AND version = ?")
        .bind(package_id)
        .bind(version)
        .execute(db)
        .await?;

    Ok(())
}

async fn total_downloads(db: &SqlitePool) -> Result<u64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(count), 0) FROM downloads")
        .fetch_one(db)
        .await?;
    Ok(count as u64)
}

async fn stats_counts(db: &SqlitePool) -> Result<(usize, usize), sqlx::Error> {
    let package_count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM packages").fetch_one(db).await?;
    let version_count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM versions").fetch_one(db).await?;
    Ok((package_count as usize, version_count as usize))
}
