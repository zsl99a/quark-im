use axum::{http::StatusCode, response::IntoResponse};

#[derive(Debug)]
pub struct Error(pub anyhow::Error);

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal Server Error: {:?}", self.0)).into_response()
    }
}

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Error(err.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
