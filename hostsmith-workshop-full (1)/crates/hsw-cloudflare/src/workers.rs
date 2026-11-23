//! Cloudflare Workers deployment (TL-04): deliberately separate from the
//! Pages static-asset flow. Script upload uses the documented module-syntax
//! PUT endpoint (metadata + module part); deletion exists but every caller
//! must apply its own explicit second confirmation.

use crate::pages::{CloudflareError, CloudflareClient};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct WorkerScript {
    pub id: String,
    pub modified_on: Option<String>,
}

impl CloudflareClient {
    /// GET /accounts/{account}/workers/scripts
    pub async fn workers_scripts(&self, account_id: &str) -> Result<Vec<WorkerScript>, CloudflareError> {
        let body = self
            .get_json(&format!("/accounts/{account_id}/workers/scripts"), &self.token)
            .await?;
        Ok(CloudflareClient::array_of(&body)
            .into_iter()
            .filter_map(|value| {
                Some(WorkerScript {
                    id: value.get("id")?.as_str()?.to_string(),
                    modified_on: value
                        .get("modified_on")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect())
    }

    /// PUT /accounts/{account}/workers/scripts/{name} - uploads an ES-module
    /// worker: a metadata part plus one module part named after the main
    /// module with an `application/javascript+module` content type.
    pub async fn upload_worker_script(
        &self,
        account_id: &str,
        name: &str,
        main_module: &str,
        code: &str,
        bindings_json: &str,
    ) -> Result<(), CloudflareError> {
        let metadata = json!({
            "main_module": main_module,
            "bindings": serde_json::from_str::<serde_json::Value>(bindings_json)
                .unwrap_or_else(|_| json!([])),
            "compatibility_date": "2024-01-01",
        });
        let url = format!("{}/accounts/{account_id}/workers/scripts/{name}", self.api_root);
        let form = reqwest::multipart::Form::new()
            .part(
                "metadata",
                reqwest::multipart::Part::text(metadata.to_string()).mime_str("application/json")?,
            )
            .part(
                main_module.to_string(),
                reqwest::multipart::Part::text(code.to_string())
                    .file_name(main_module.to_string())
                    .mime_str("application/javascript+module")?,
            );
        let response = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await?;
        self.envelope(response).await?;
        Ok(())
    }

    /// DELETE /accounts/{account}/workers/scripts/{name}. The interface must
    /// never call this without its own explicit, separately-confirmed dialog
    /// (spec Section 15: deletion is a separate, separately-confirmed step).
    pub async fn delete_worker_script(&self, account_id: &str, name: &str) -> Result<(), CloudflareError> {
        let url = format!("{}/accounts/{account_id}/workers/scripts/{name}", self.api_root);
        let response = self.http.delete(&url).bearer_auth(&self.token).send().await?;
        self.envelope(response).await?;
        Ok(())
    }
}
