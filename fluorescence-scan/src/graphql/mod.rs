/// Collection of graphql entities
mod entities;
use async_graphql::{
    ComplexObject, Context, EmptyMutation, EmptySubscription, Object, Schema, SchemaBuilder,
};
use entities::{FluorescenceScan, Session};
use models::xfe_fluorescence_spectrum;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

/// The GraphQL schema exposed by the service
pub type RootSchema = Schema<Query, EmptyMutation, EmptySubscription>;

/// A schema builder for the service
pub fn root_schema_builder() -> SchemaBuilder<Query, EmptyMutation, EmptySubscription> {
    Schema::build(Query, EmptyMutation, EmptySubscription).enable_federation()
}

/// The root query of the service
#[derive(Debug, Clone, Default)]
pub struct Query;

#[ComplexObject]
impl Session {
    /// Fetched all crystal snapshots and generates s3 URLs
    async fn fluorescence_scan(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<FluorescenceScan>> {
        let database = ctx.data::<DatabaseConnection>()?;
        Ok(xfe_fluorescence_spectrum::Entity::find()
            .filter(xfe_fluorescence_spectrum::Column::SessionId.eq(self.id))
            .all(database)
            .await?
            .into_iter()
            .map(FluorescenceScan::from)
            .collect())
    }
}

#[Object]
impl Query {
    /// Reference datasets resolver for the router
    #[graphql(entity)]
    async fn router_session(&self, id: u32) -> Session {
        Session { id }
    }
}
