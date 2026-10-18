//! The permission catalog, backed by SeaORM.

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::domain::errors::RepositoryError;
use crate::domain::rbac::{Permission, PermissionRepository};

use super::entities::permissions;
use super::error::{map_db_err, not_found};
use super::mapping::from_utc;

const ENTITY: &str = "permission";
const CONFLICT: &str = "a permission with that name already exists";

/// The permission catalog, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmPermissionRepository {
    db: DatabaseConnection,
}

impl SeaOrmPermissionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl PermissionRepository for SeaOrmPermissionRepository {
    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>, RepositoryError> {
        let model = permissions::Entity::find()
            .filter(permissions::Column::Name.eq(name))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(Permission::from))
    }

    async fn list(&self) -> Result<Vec<Permission>, RepositoryError> {
        let models = permissions::Entity::find()
            .order_by_asc(permissions::Column::Group)
            .order_by_asc(permissions::Column::Name)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(models.into_iter().map(Permission::from).collect())
    }

    async fn upsert(
        &self,
        name: &str,
        group: &str,
        description: &str,
    ) -> Result<Permission, RepositoryError> {
        // Bootstrap calls this on every start, so it has to be an upsert on the
        // name: a redeploy that edits a description must not fail on the unique
        // index, and must not leave the old wording behind either.
        let model = permissions::ActiveModel {
            id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
            name: sea_orm::ActiveValue::Set(name.to_string()),
            group: sea_orm::ActiveValue::Set(group.to_string()),
            description: sea_orm::ActiveValue::Set(description.to_string()),
            created_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
        };

        permissions::Entity::insert(model)
            .on_conflict(
                OnConflict::column(permissions::Column::Name)
                    // The group and the description are ours to revise; the name is
                    // the key every check references, so it is never the thing updated.
                    .update_columns([permissions::Column::Group, permissions::Column::Description])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        self.find_by_name(name)
            .await?
            .ok_or_else(|| not_found(ENTITY))
    }
}

impl From<permissions::Model> for Permission {
    fn from(model: permissions::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            group: model.group,
            description: model.description,
        }
    }
}
