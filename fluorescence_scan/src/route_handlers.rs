use async_graphql::Executor;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::Request,
    handler::Handler,
    http::StatusCode,
    response::{IntoResponse, Response},
    RequestExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use std::{future::Future, pin::Pin};

/// An [`Handler`] which executes an [`Executor`] including the [`Authorization<Bearer>`] in the [`async_graphql::Context`]
#[derive(Debug, Clone)]
pub struct GraphQLHandler<E: Executor> {
    /// The GraphQL executor used to process the request
    executor: E,
}

impl<E: Executor> GraphQLHandler<E> {
    /// Constructs an instance of the handler with the provided schema.
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<S, E> Handler<((),), S> for GraphQLHandler<E>
where
    E: Executor,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send + 'static>>;

    fn call(self, mut req: Request, _state: S) -> Self::Future {
        Box::pin(async move {
            let token = req
                .extract_parts::<TypedHeader<Authorization<Bearer>>>()
                .await
                .ok()
                .map(|token| token.0);
            let request = req.extract::<GraphQLRequest, _>().await;
            match request {
                Ok(request) => GraphQLResponse::from(
                    self.executor
                        .execute(request.into_inner().data(token))
                        .await,
                )
                .into_response(),
                Err(err) => (StatusCode::BAD_REQUEST, err.0.to_string()).into_response(),
            }
        })
    }
}
