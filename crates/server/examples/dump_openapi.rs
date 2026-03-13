use serverbee_server::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() {
    print!(
        "{}",
        ApiDoc::openapi()
            .to_pretty_json()
            .expect("Failed to serialize OpenAPI spec")
    );
}
