use askama::Template;
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::Html,
    routing::get,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::env;

#[derive(Clone)]
struct ExplorerState {
    http: Client,
    rpc_url: String,
}

#[derive(Deserialize)]
struct AddressQuery {
    address: String,
}

#[derive(Template)]
#[template(
    source = r###"
<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="utf-8">
    <title>Prims Explorer</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 960px; margin: 40px auto; padding: 0 16px; line-height: 1.5; }
        h1, h2 { margin-bottom: 0.4em; }
        .card { border: 1px solid #ddd; border-radius: 12px; padding: 16px; margin: 16px 0; }
        input[type="text"] { width: 100%; padding: 10px; box-sizing: border-box; margin: 8px 0 12px; }
        button { padding: 10px 14px; cursor: pointer; }
        pre { white-space: pre-wrap; word-break: break-word; background: #f6f8fa; padding: 12px; border-radius: 10px; overflow-x: auto; }
        .muted { color: #666; }
    </style>
</head>
<body>
    <h1>Prims Explorer</h1>
    <p class="muted">Endpoint RPC utilisé : <strong>{{ rpc_url }}</strong></p>

    <div class="card">
        <h2>Recherche de solde</h2>
        <form action="/address" method="get">
            <label for="address">Adresse</label>
            <input id="address" name="address" type="text" placeholder="Colle une adresse publique" required>
            <button type="submit">Voir le solde</button>
        </form>
    </div>

    <div class="card">
        <h2>Informations du nœud</h2>
        <pre>{{ info_json }}</pre>
    </div>

    <div class="card">
        <h2>Validateurs</h2>
        <pre>{{ validators_json }}</pre>
    </div>

    <div class="card">
        <h2>Commitments anonymes</h2>
        <pre>{{ note_commitments_json }}</pre>
    </div>
</body>
</html>
"###,
    ext = "html"
)]
struct HomeTemplate {
    rpc_url: String,
    info_json: String,
    validators_json: String,
    note_commitments_json: String,
}

#[derive(Template)]
#[template(
    source = r###"
<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="utf-8">
    <title>Prims Explorer - Adresse</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 960px; margin: 40px auto; padding: 0 16px; line-height: 1.5; }
        .card { border: 1px solid #ddd; border-radius: 12px; padding: 16px; margin: 16px 0; }
        pre { white-space: pre-wrap; word-break: break-word; background: #f6f8fa; padding: 12px; border-radius: 10px; overflow-x: auto; }
        a { text-decoration: none; }
        .muted { color: #666; }
    </style>
</head>
<body>
    <p><a href="/">← Retour à l’accueil</a></p>
    <h1>Solde d’une adresse</h1>
    <p class="muted">Endpoint RPC utilisé : <strong>{{ rpc_url }}</strong></p>

    <div class="card">
        <h2>Adresse</h2>
        <pre>{{ address }}</pre>
    </div>

    <div class="card">
        <h2>Réponse RPC get_balance</h2>
        <pre>{{ balance_json }}</pre>
    </div>
</body>
</html>
"###,
    ext = "html"
)]
struct AddressTemplate {
    rpc_url: String,
    address: String,
    balance_json: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind_address =
        env::var("PRIMS_EXPLORER_ADDRESS").unwrap_or_else(|_| "127.0.0.1:7003".to_string());
    let rpc_url = env::var("PRIMS_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:7002".to_string());

    let state = ExplorerState {
        http: Client::builder().build()?,
        rpc_url: rpc_url.clone(),
    };

    let app = Router::new()
        .route("/", get(home))
        .route("/address", get(address))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    println!(
        "PRIMS explorer listening on http://{}",
        listener.local_addr()?
    );
    println!("Connected RPC endpoint: {}", rpc_url);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn home(State(state): State<ExplorerState>) -> Result<Html<String>, (StatusCode, String)> {
    let info_json =
        pretty_json_or_error(rpc_call(&state.http, &state.rpc_url, "get_info", json!({})).await);
    let validators_json = pretty_json_or_error(
        rpc_call(&state.http, &state.rpc_url, "get_validators", json!({})).await,
    );
    let note_commitments_json = pretty_json_or_error(
        rpc_call(
            &state.http,
            &state.rpc_url,
            "get_note_commitments",
            json!({}),
        )
        .await,
    );

    render_template(&HomeTemplate {
        rpc_url: state.rpc_url,
        info_json,
        validators_json,
        note_commitments_json,
    })
}

async fn address(
    State(state): State<ExplorerState>,
    Query(query): Query<AddressQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let address = query.address;

    let balance_json = pretty_json_or_error(
        rpc_call(
            &state.http,
            &state.rpc_url,
            "get_balance",
            json!({ "address": address.clone() }),
        )
        .await,
    );

    render_template(&AddressTemplate {
        rpc_url: state.rpc_url,
        address,
        balance_json,
    })
}

async fn rpc_call(
    http: &Client,
    rpc_url: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to decode JSON-RPC response: {e}"))?;

    if let Some(result) = payload.get("result") {
        Ok(result.clone())
    } else if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string());
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown RPC error");
        Err(format!("RPC error {code}: {message}"))
    } else {
        Err(format!(
            "Unexpected JSON-RPC payload (HTTP {status}): {}",
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "<payload non affichable>".to_string())
        ))
    }
}

fn pretty_json_or_error(result: Result<Value, String>) -> String {
    match result {
        Ok(value) => serde_json::to_string_pretty(&value)
            .unwrap_or_else(|_| "<résultat JSON non affichable>".to_string()),
        Err(error) => format!("Erreur: {error}"),
    }
}

fn render_template<T: Template>(template: &T) -> Result<Html<String>, (StatusCode, String)> {
    template.render().map(Html).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template render error: {e}"),
        )
    })
}
