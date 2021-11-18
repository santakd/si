use crate::service_tests::{api_request_auth_json_body, api_request_auth_query};
use crate::test_setup;
use axum::http::Method;
use dal::{HistoryActor, SchemaKind, StandardModel, Tenancy, Visibility};
use sdf::service::schema::create_schema::{CreateSchemaRequest, CreateSchemaResponse};
use dal::test_harness::create_schema as dal_create_schema;
use sdf::service::schema::list_schemas::{ListSchemaRequest, ListSchemaResponse};

#[tokio::test]
async fn create_schema() {
    test_setup!(
        _ctx,
        _secret_key,
        _pg,
        _conn,
        _txn,
        _nats_conn,
        _nats,
        app,
        _nba,
        auth_token
    );
    let request = CreateSchemaRequest {
        name: "fancyPants".to_string(),
        kind: SchemaKind::Concrete,
    };
    let response: CreateSchemaResponse = api_request_auth_json_body(
        app,
        Method::POST,
        "/api/schema/create_schema",
        &auth_token,
        &request,
    )
    .await;
    assert_eq!(response.schema.name(), "fancyPants");
    assert_eq!(response.schema.kind(), &SchemaKind::Concrete);
}

#[tokio::test]
async fn list_schemas() {
    test_setup!(
        _ctx,
        _secret_key,
        _pg,
        _conn,
        txn,
        _nats_conn,
        nats,
        app,
        nba,
        auth_token
    );
    let tenancy = Tenancy::new_billing_account(vec![*nba.billing_account.id()]);
    let visibility = Visibility::new_head(false);
    let history_actor = HistoryActor::SystemInit;
    let schema_one = dal_create_schema(&txn, &nats, &tenancy, &visibility, &history_actor, &SchemaKind::Concrete).await;
    let schema_two = dal_create_schema(&txn, &nats, &tenancy, &visibility, &history_actor, &SchemaKind::Concrete).await;
    txn.commit().await.expect("cannot commit txn");
    let request = ListSchemaRequest { visibility: visibility.clone() };
    let response: ListSchemaResponse = api_request_auth_query(app, "/api/schema/list_schemas", &auth_token, &request).await;
    assert_eq!(response.list.len(), 2);
}
