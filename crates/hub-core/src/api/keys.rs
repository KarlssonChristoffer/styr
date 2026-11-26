//! Routes for the keys area: the vault, agent keys, API connections and customs.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/secrets", get(list_secrets))
        .route("/api/keys", get(key_vault).post(create_key))
        .route("/api/keys/{name}", axum::routing::put(rotate_key).delete(delete_secret))
        .route("/api/apis", get(api_state).put(set_api_prefs))
        .route("/api/apis/decisions", get(api_decisions))
        .route("/api/apis/tune", post(jev_tune))
        .route("/api/apis/tune/run", post(|State(hub): State<Arc<Hub>>| async move { hub.start_tune(); Json(hub.tune_state().await.unwrap_or_default()) }))
        .route("/api/apis/tune/state", get(|State(hub): State<Arc<Hub>>| async move { Ok::<_, ApiError>(Json(hub.tune_state().await?)) }))
        .route("/api/apis/tune/answer", post(answer_tune))
        .route("/api/apis/{name}/test", post(test_api))
        .route("/api/apis/{name}/try", post(try_api))
        .route("/api/apis/{name}/login", post(|State(hub): State<Arc<Hub>>, Path(name): Path<String>| async move {
            ApiResult::Ok(Json(serde_json::json!({ "url": hub.api_login(&name).await? })))
        }))
        .route("/api/secrets/{name}", axum::routing::put(set_secret).delete(delete_secret))
        .route("/api/customs", get(list_customs).put(edit_customs).delete(remove_customs))
        .route("/api/customs/import", post(import_customs))
        .route("/api/customs/review", post(review_customs))
        .route("/api/customs/repo", post(check_customs_repo))
        .route("/api/customs/activity", get(customs_activity))
        .route("/api/customs/prefs", get(customs_prefs).put(set_customs_prefs))
        .route("/api/customs/share", post(share_customs))
        .route("/api/customs/registry", get(customs_registry_info))
        .route("/api/agent-keys", get(list_agent_keys).post(create_agent_key))
        .route("/api/agent-keys/{id}", axum::routing::delete(revoke_agent_key))
}

async fn api_state(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.api_state().await?))
}

async fn set_api_prefs(State(hub): State<Arc<Hub>>, Json(prefs): Json<crate::apis::ApiPrefs>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_api_prefs(prefs).await?))
}

async fn api_decisions(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.api_decisions().await?))
}

async fn jev_tune(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.jev_tune().await?))
}

async fn answer_tune(State(hub): State<Arc<Hub>>, Json(a): Json<TuneAnswer>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.answer_tune(&a.id, a.used).await?))
}

async fn test_api(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.test_api(&name).await?))
}

async fn try_api(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(ask): Json<crate::apis::ApiTry>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.try_api(&name, &ask).await?))
}

async fn list_secrets(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.secrets.list().await?))
}

async fn set_secret(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(body): Json<crate::model::SetSecret>,
) -> ApiResult<impl IntoResponse> {
    hub.secrets.set(&name, &body.value).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn key_vault(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.key_vault().await?))
}

async fn create_key(State(hub): State<Arc<Hub>>, Json(new): Json<crate::model::NewSecret>) -> ApiResult<impl IntoResponse> {
    hub.secrets.create(&new.name, &new.value).await?;
    Ok(Json(hub.key_vault().await?))
}

async fn rotate_key(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(body): Json<crate::model::SetSecret>,
) -> ApiResult<impl IntoResponse> {
    hub.secrets.rotate(&name, &body.value).await?;
    Ok(Json(hub.key_vault().await?))
}

async fn delete_secret(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.secrets.delete(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_customs(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs::list(&hub.ctx()).await?))
}

async fn edit_customs(
    State(hub): State<Arc<Hub>>,
    Json(edit): Json<crate::model::CustomsEdit>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs::edit(&hub.ctx(), edit).await?))
}

async fn remove_customs(State(hub): State<Arc<Hub>>, Query(m): Query<CustomsRef>) -> ApiResult<impl IntoResponse> {
    crate::customs::remove(&hub.ctx(), &m.kind, &m.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn import_customs(State(hub): State<Arc<Hub>>, Json(body): Json<ImportBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs::import(&hub.ctx(), std::path::Path::new(&body.folder)).await?))
}

async fn review_customs(State(hub): State<Arc<Hub>>, Json(m): Json<CustomsRef>) -> ApiResult<impl IntoResponse> {
    let item = crate::customs_review::review(&hub, &m.kind, &m.name).await?;
    Ok(Json(item))
}

/// Puts a GitHub repository through Customs now and answers its item: Cleared, Held or Refused.
async fn check_customs_repo(State(hub): State<Arc<Hub>>, Json(body): Json<CustomsRepoBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs_review::repo::check(&hub, &body.repo, None).await?))
}

/// What happens at Customs: the last month of its log, or all of one item's, and the reviews
/// running now.
async fn customs_activity(State(hub): State<Arc<Hub>>, Query(q): Query<CustomsActivityQuery>) -> ApiResult<impl IntoResponse> {
    let item = q.kind.zip(q.name).map(|(k, n)| {
        let n = crate::customs::normalize(&k, &n);
        (k, n)
    });
    Ok(Json(crate::customs_log::activity(&hub.store, item).await?))
}

async fn share_customs(State(hub): State<Arc<Hub>>, Json(theirs): Json<crate::customs::Shared>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs::merge(&hub.ctx(), theirs).await?))
}

async fn customs_registry_info(State(hub): State<Arc<Hub>>) -> Json<crate::model::CustomsRegistry> {
    let base = crate::registry_address::base(&hub.config);
    Json(crate::model::CustomsRegistry {
        cargo_config: crate::registry_address::cargo_config(&base),
        env: crate::registry_address::env(&base).into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        base,
    })
}

async fn customs_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::customs::prefs(&hub.ctx()).await?))
}

async fn set_customs_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::model::CustomsPrefs>,
) -> ApiResult<impl IntoResponse> {
    crate::customs::set_prefs(&hub.ctx(), &prefs).await?;
    Ok(Json(prefs))
}

async fn list_agent_keys(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::hub_tools::keys(&hub).await?))
}

async fn create_agent_key(
    State(hub): State<Arc<Hub>>,
    Json(new): Json<crate::model::NewAgentKey>,
) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::CREATED, Json(crate::hub_tools::create_key(&hub, new).await?)))
}

async fn revoke_agent_key(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    crate::hub_tools::revoke_key(&hub, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
