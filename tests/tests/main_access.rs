use function_name::named;
use reqwest::StatusCode;
use tests::laplace_service::env;
use tests::{init_logger, LaplaceService};

#[tokio::test]
#[named]
async fn http_access() {
    init_logger();

    let service = LaplaceService::new(function_name!())
        .with_var(env::SSL_ENABLED, "false")
        .start();
    let client = service.http_client().await;

    let response = client.get_index().await.expect("Cannot get index");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[named]
async fn https_access() {
    init_logger();

    let service = LaplaceService::new(function_name!())
        .with_var(env::SSL_ENABLED, "true")
        .start();
    let client = service.https_client().await;

    let response = client.get_index().await.expect("Cannot get index");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[named]
async fn unauthorized_access_denied() {
    let service = LaplaceService::new(function_name!()).start();
    let client = service.https_client().await;

    let response = client.get_index().await.expect("Fail to get index");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = client.get_laplace().await.expect("Fail to get laplace");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
