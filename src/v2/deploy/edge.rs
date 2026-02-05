//! Edge Deployment Integrations
//!
//! Providers: Cloudflare Workers, Fastly Compute@Edge, AWS Lambda, Vercel Edge

use crate::v2::{Error, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeProvider {
    Cloudflare,
    Fastly,
    AwsLambda,
    Vercel,
}

impl EdgeProvider {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "cloudflare" | "cf" | "workers" => Ok(Self::Cloudflare),
            "fastly" | "compute" | "compute@edge" => Ok(Self::Fastly),
            "aws" | "lambda" | "aws-lambda" => Ok(Self::AwsLambda),
            "vercel" | "vercel-edge" => Ok(Self::Vercel),
            other => Err(Error::other(format!("Unknown edge provider '{}'", other))),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::Fastly => "fastly",
            Self::AwsLambda => "aws-lambda",
            Self::Vercel => "vercel",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EdgeDeployment {
    pub provider: EdgeProvider,
    pub name: String,
    pub component_path: String,
    pub options: HashMap<String, String>,
}

pub async fn deploy_edge(deployment: EdgeDeployment) -> Result<String> {
    match deployment.provider {
        EdgeProvider::Cloudflare => deploy_cloudflare(deployment).await,
        EdgeProvider::Fastly => deploy_fastly(deployment).await,
        EdgeProvider::AwsLambda => deploy_aws_lambda(deployment).await,
        EdgeProvider::Vercel => deploy_vercel(deployment).await,
    }
}

async fn deploy_cloudflare(deployment: EdgeDeployment) -> Result<String> {
    let account_id = deployment
        .options
        .get("account_id")
        .ok_or_else(|| Error::other("account_id required for Cloudflare"))?;

    let api_token = deployment
        .options
        .get("api_token")
        .map(|s| s.clone())
        .or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok())
        .ok_or_else(|| Error::other("api_token or CLOUDFLARE_API_TOKEN required"))?;

    // Use wrangler CLI
    let output = Command::new("wrangler")
        .args([
            "deploy",
            &deployment.component_path,
            "--name",
            &deployment.name,
        ])
        .env("CLOUDFLARE_ACCOUNT_ID", account_id)
        .env("CLOUDFLARE_API_TOKEN", api_token)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let url = format!("https://{}.workers.dev", deployment.name);
            Ok(url)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(Error::other(format!(
                "Cloudflare deploy failed: {}",
                stderr
            )))
        }
        Err(e) => Err(Error::other(format!("wrangler command failed: {}", e))),
    }
}

async fn deploy_fastly(deployment: EdgeDeployment) -> Result<String> {
    let service_id = deployment
        .options
        .get("service_id")
        .ok_or_else(|| Error::other("service_id required for Fastly"))?;

    let output = Command::new("fastly")
        .args([
            "compute",
            "deploy",
            "--service-id",
            service_id,
            "--path",
            &deployment.component_path,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let url = format!("https://{}.edgecompute.app", deployment.name);
            Ok(url)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(Error::other(format!("Fastly deploy failed: {}", stderr)))
        }
        Err(e) => Err(Error::other(format!("fastly command failed: {}", e))),
    }
}

async fn deploy_aws_lambda(deployment: EdgeDeployment) -> Result<String> {
    let function_name = &deployment.name;
    let region = deployment
        .options
        .get("region")
        .map(|s| s.as_str())
        .unwrap_or("us-east-1");
    let role_arn = deployment
        .options
        .get("role_arn")
        .ok_or_else(|| Error::other("role_arn required for AWS Lambda"))?;

    // Package as zip
    let temp_dir = std::env::temp_dir().join(format!("run-deploy-{}", function_name));
    std::fs::create_dir_all(&temp_dir)?;

    let bootstrap_path = temp_dir.join("bootstrap");
    create_lambda_bootstrap(&bootstrap_path, &deployment.component_path)?;

    let zip_path = temp_dir.join("function.zip");
    create_lambda_zip(&temp_dir, &zip_path)?;

    // Deploy via AWS CLI
    let output = Command::new("aws")
        .args([
            "lambda",
            "create-function",
            "--function-name",
            function_name,
            "--runtime",
            "provided.al2023",
            "--role",
            role_arn,
            "--zip-file",
            &format!("fileb://{}", zip_path.display()),
            "--region",
            region,
            "--architecture",
            "x86_64",
        ])
        .output();

    std::fs::remove_dir_all(&temp_dir)?;

    match output {
        Ok(out) if out.status.success() => {
            let url = format!("https://{}.lambda-url.{}.on.aws/", function_name, region);
            Ok(url)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("ResourceConflictException") {
                // Update existing function
                let update = Command::new("aws")
                    .args([
                        "lambda",
                        "update-function-code",
                        "--function-name",
                        function_name,
                        "--zip-file",
                        &format!("fileb://{}", zip_path.display()),
                        "--region",
                        region,
                    ])
                    .output();

                match update {
                    Ok(out) if out.status.success() => {
                        let url =
                            format!("https://{}.lambda-url.{}.on.aws/", function_name, region);
                        Ok(url)
                    }
                    _ => Err(Error::other(format!(
                        "AWS Lambda deploy failed: {}",
                        stderr
                    ))),
                }
            } else {
                Err(Error::other(format!(
                    "AWS Lambda deploy failed: {}",
                    stderr
                )))
            }
        }
        Err(e) => Err(Error::other(format!("aws command failed: {}", e))),
    }
}

async fn deploy_vercel(deployment: EdgeDeployment) -> Result<String> {
    let output = Command::new("vercel")
        .args(["deploy", &deployment.component_path, "--prod"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let url = stdout
                .lines()
                .find(|line| line.starts_with("https://"))
                .map(|s| s.trim().to_string())
                .ok_or_else(|| Error::other("Could not parse Vercel deployment URL"))?;
            Ok(url)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(Error::other(format!("Vercel deploy failed: {}", stderr)))
        }
        Err(e) => Err(Error::other(format!("vercel command failed: {}", e))),
    }
}

fn create_lambda_bootstrap(bootstrap_path: &Path, component_path: &str) -> Result<()> {
    let bootstrap_script = format!(
        r#"#!/bin/sh
exec wasmtime run --invoke handler {} "$@"
"#,
        component_path
    );
    std::fs::write(bootstrap_path, bootstrap_script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(bootstrap_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(bootstrap_path, perms)?;
    }

    Ok(())
}

fn create_lambda_zip(source_dir: &Path, output_path: &Path) -> Result<()> {
    let output = Command::new("zip")
        .args([
            "-j",
            output_path.to_str().unwrap(),
            source_dir.join("bootstrap").to_str().unwrap(),
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(Error::other(format!("zip failed: {}", stderr)))
        }
        Err(e) => Err(Error::other(format!("zip command not found: {}", e))),
    }
}

#[derive(Debug, Serialize)]
pub struct EdgeDeploymentManifest {
    pub name: String,
    pub provider: String,
    pub url: String,
    pub deployed_at: u64,
    pub component: String,
    pub sha256: String,
}

pub fn generate_edge_manifest(
    name: &str,
    provider: EdgeProvider,
    url: &str,
    component_path: &Path,
    sha256: &str,
) -> EdgeDeploymentManifest {
    EdgeDeploymentManifest {
        name: name.to_string(),
        provider: provider.name().to_string(),
        url: url.to_string(),
        deployed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        component: component_path.display().to_string(),
        sha256: sha256.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_provider_parse() {
        assert!(matches!(
            EdgeProvider::from_str("cloudflare").unwrap(),
            EdgeProvider::Cloudflare
        ));
        assert!(matches!(
            EdgeProvider::from_str("aws-lambda").unwrap(),
            EdgeProvider::AwsLambda
        ));
    }
}
