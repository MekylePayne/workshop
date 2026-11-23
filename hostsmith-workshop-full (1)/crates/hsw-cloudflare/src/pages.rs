//! Native Cloudflare Pages deployment client (TL-03, spec Section 15).
//!
//! Primary implementation: the documented REST asset-upload flow -
//! fetch a project upload token, check which asset hashes are missing,
//! upload only the missing assets, upsert the hashes, create the deployment
//! with the manifest. No Wrangler, no Node.js. Progress is reported through
//! a callback at every step so the interface can stream deployment status.
//!
//! Auth: account-level requests use the scoped API token; asset requests
//! use the short-lived upload token (JWT) exactly as the endpoints require.

use crate::manifest::{self, UploadFile};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

/// Base64 payload bytes per upload request (endpoint limit ~50 MB; we stay
/// safely below it).
const UPLOAD_BUCKET_BYTES: usize = 48 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CloudflareError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Cloudflare API error ({status}): {messages}")]
    Api { status: u16, messages: String },
    #[error("the upload-token response did not contain a JWT string")]
    MissingJwt,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PagesProject {
    pub name: String,
    pub subdomain: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployOutcome {
    pub url: Option<String>,
    pub total_files: usize,
    pub uploaded: usize,
    pub already_present: usize,
}

pub type Progress<'a> = &'a (dyn Fn(DeployStep, usize, usize) + Send + Sync);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployStep {
    Hashing,
    Token,
    Checking,
    Uploading,
    Upserting,
    CreatingDeployment,
    Done,
}

pub struct CloudflareClient {
    pub(crate) http: reqwest::Client,
    pub(crate) token: String,
    pub(crate) api_root: String,
}

impl CloudflareClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_api_root(token, "https://api.cloudflare.com/client/v4")
    }

    /// Test/development constructor with a custom API root.
    pub fn with_api_root(token: impl Into<String>, api_root: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), token: token.into(), api_root: api_root.into() }
    }

    pub(crate) async fn envelope(&self, response: reqwest::Response) -> Result<serde_json::Value, CloudflareError> {
        let status = response.status().as_u16();
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|err| CloudflareError::Api { status, messages: format!("non-JSON response: {err}") })?;
        let success = body.get("success").and_then(serde_json::Value::as_bool).unwrap_or(false);
        if !success || status >= 400 {
            let messages = body
                .get("errors")
                .map(|errors| errors.to_string())
                .unwrap_or_else(|| body.to_string());
            return Err(CloudflareError::Api { status, messages });
        }
        Ok(body)
    }

    pub(crate) async fn get_json(&self, path: &str, bearer: &str) -> Result<serde_json::Value, CloudflareError> {
        let url = format!("{}{path}", self.api_root);
        let response = self.http.get(&url).bearer_auth(bearer).send().await?;
        self.envelope(response).await
    }

    async fn post_json(
        &self,
        path: &str,
        bearer: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, CloudflareError> {
        let url = format!("{}{path}", self.api_root);
        let response = self
            .http
            .post(&url)
            .bearer_auth(bearer)
            .json(&body)
            .send()
            .await?;
        self.envelope(response).await
    }

    pub(crate) fn array_of(body: &serde_json::Value) -> Vec<serde_json::Value> {
        body.get("result")
            .and_then(|result| result.as_array())
            .cloned()
            .unwrap_or_default()
    }

    /// GET /accounts - the accounts the token can see.
    pub async fn accounts(&self) -> Result<Vec<Account>, CloudflareError> {
        let body = self.get_json("/accounts", &self.token).await?;
        Ok(Self::array_of(&body)
            .into_iter()
            .filter_map(|value| {
                Some(Account {
                    id: value.get("id")?.as_str()?.to_string(),
                    name: value.get("name").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// GET /accounts/{account}/pages/projects
    pub async fn pages_projects(&self, account_id: &str) -> Result<Vec<PagesProject>, CloudflareError> {
        let body = self
            .get_json(&format!("/accounts/{account_id}/pages/projects"), &self.token)
            .await?;
        Ok(Self::array_of(&body)
            .into_iter()
            .filter_map(|value| {
                Some(PagesProject {
                    name: value.get("name")?.as_str()?.to_string(),
                    subdomain: value
                        .get("subdomain")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect())
    }

    /// POST /accounts/{account}/pages/projects - creation where supported.
    pub async fn create_pages_project(
        &self,
        account_id: &str,
        name: &str,
        production_branch: &str,
    ) -> Result<PagesProject, CloudflareError> {
        let body = self
            .post_json(
                &format!("/accounts/{account_id}/pages/projects"),
                &self.token,
                json!({ "name": name, "production_branch": production_branch }),
            )
            .await?;
        let result = body.get("result").cloned().unwrap_or_default();
        Ok(PagesProject {
            name: result.get("name").and_then(serde_json::Value::as_str).unwrap_or(name).to_string(),
            subdomain: result.get("subdomain").and_then(serde_json::Value::as_str).map(str::to_string),
        })
    }

    /// GET /accounts/{account}/pages/projects/{project}/upload-token
    pub async fn upload_token(&self, account_id: &str, project: &str) -> Result<String, CloudflareError> {
        let body = self
            .get_json(
                &format!("/accounts/{account_id}/pages/projects/{project}/upload-token"),
                &self.token,
            )
            .await?;
        body.get("result")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or(CloudflareError::MissingJwt)
    }

    /// POST /accounts/{account}/pages/assets/check-missing (upload JWT).
    pub async fn check_missing(
        &self,
        account_id: &str,
        jwt: &str,
        hashes: &[String],
    ) -> Result<Vec<String>, CloudflareError> {
        let body = self
            .post_json(
                &format!("/accounts/{account_id}/pages/assets/check-missing"),
                jwt,
                json!(hashes),
            )
            .await?;
        Ok(body
            .get("result")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect())
    }

    /// POST /pages/assets/upload (upload JWT): JSON array of
    /// { key, value (base64), metadata: { contentType }, base64: true },
    /// bucketed to stay below the ~50 MB request limit.
    pub async fn upload_assets(
        &self,
        jwt: &str,
        files: &[UploadFile],
        progress: Progress<'_>,
    ) -> Result<(), CloudflareError> {
        let total = files.len();
        let mut uploaded = 0usize;
        for bucket in manifest::bucket_files(files, UPLOAD_BUCKET_BYTES) {
            let payload: Vec<serde_json::Value> = bucket
                .iter()
                .map(|file| {
                    json!({
                        "key": file.hash,
                        "value": B64.encode(&file.contents),
                        "metadata": { "contentType": file.content_type },
                        "base64": true,
                    })
                })
                .collect();
            let url = format!("{}/pages/assets/upload", self.api_root);
            let response = self.http.post(&url).bearer_auth(jwt).json(&payload).send().await?;
            self.envelope(response).await?;
            uploaded += bucket.len();
            progress(DeployStep::Uploading, uploaded, total);
        }
        Ok(())
    }

    /// POST /accounts/{account}/pages/assets/upsert-hashes (upload JWT).
    pub async fn upsert_hashes(
        &self,
        account_id: &str,
        jwt: &str,
        hashes: &[String],
    ) -> Result<(), CloudflareError> {
        self.post_json(
            &format!("/accounts/{account_id}/pages/assets/upsert-hashes"),
            jwt,
            json!({ "hashes": hashes }),
        )
        .await?;
        Ok(())
    }

    /// POST /accounts/{account}/pages/projects/{project}/deployments with
    /// the `manifest` multipart field (plus an optional branch).
    pub async fn create_deployment(
        &self,
        account_id: &str,
        project: &str,
        manifest_body: &str,
        branch: Option<&str>,
    ) -> Result<Option<String>, CloudflareError> {
        let url = format!("{}/accounts/{account_id}/pages/projects/{project}/deployments", self.api_root);
        let mut form = reqwest::multipart::Form::new().part(
            "manifest",
            reqwest::multipart::Part::text(manifest_body.to_string())
                .file_name("manifest")
                .mime_str("application/json")?,
        );
        if let Some(branch) = branch {
            form = form.part("branch", reqwest::multipart::Part::text(branch.to_string()));
        }
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await?;
        let body = self.envelope(response).await?;
        Ok(body
            .get("result")
            .and_then(|result| result.get("url"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string))
    }

    /// The full documented deployment flow against one directory. Files are
    /// never modified; only their contents are hashed and uploaded.
    pub async fn deploy_directory(
        &self,
        account_id: &str,
        project: &str,
        root: &Path,
        progress: Progress<'_>,
    ) -> Result<DeployOutcome, CloudflareError> {
        progress(DeployStep::Hashing, 0, 0);
        let files = manifest::build_from_directory(root)?;
        let total = files.len();
        if total == 0 {
            return Err(CloudflareError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the project directory contains no files to deploy",
            )));
        }

        progress(DeployStep::Token, 0, total);
        let jwt = self.upload_token(account_id, project).await?;

        progress(DeployStep::Checking, 0, total);
        let hashes: Vec<String> = files.iter().map(|file| file.hash.clone()).collect();
        let missing: HashSet<String> = self.check_missing(account_id, &jwt, &hashes).await?.into_iter().collect();

        let mut to_upload = Vec::new();
        let mut already_present = Vec::new();
        for file in files {
            if missing.contains(&file.hash) {
                to_upload.push(file);
            } else {
                already_present.push(file);
            }
        }

        self.upload_assets(&jwt, &to_upload, progress).await?;

        progress(DeployStep::Upserting, to_upload.len(), total);
        let mut all_files = to_upload.clone();
        all_files.extend(already_present.iter().cloned());
        let all_hashes: Vec<String> = all_files.iter().map(|file| file.hash.clone()).collect();
        self.upsert_hashes(account_id, &jwt, &all_hashes).await?;

        progress(DeployStep::CreatingDeployment, total, total);
        let manifest_body = manifest::manifest_json(&all_files);
        let url = self.create_deployment(account_id, project, &manifest_body, None).await?;

        progress(DeployStep::Done, total, total);
        Ok(DeployOutcome {
            url,
            total_files: total,
            uploaded: to_upload.len(),
            already_present: already_present.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_construction_is_trivial() {
        let client = CloudflareClient::new("test-token");
        assert_eq!(client.token, "test-token");
        assert!(client.api_root.ends_with("/client/v4"));
    }
}
